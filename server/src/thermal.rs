//! Processor temperatures used for display and fan curves.
//!
//! EC register `0x70` is the ACPI CPUT byte. On current EVO-X2 firmware that
//! value does not match AMD's sensors (it often sits near 97°C). Prefer AMD
//! driver readings: WDDM GPU (Task Manager), then ADL GPU / CPU / SoC /
//! hotspot. Fan curves and alerts follow the sensor selected in Settings
//! (GPU by default).

use std::ffi::c_void;
use std::mem::{size_of, MaybeUninit};
use std::ptr;
use std::sync::{Mutex, OnceLock};

use winapi::shared::minwindef::HMODULE;
use winapi::um::libloaderapi::GetProcAddress;

use crate::harden::load_system32_library;

const AMD_VENDOR_ID: u32 = 0x1002;
const KMTQAITYPE_ADAPTERTYPE: i32 = 15;
const KMTQAITYPE_PHYSICALADAPTERDEVICEIDS: i32 = 31;
const KMTQAITYPE_ADAPTERPERFDATA: i32 = 62;
const ADAPTER_TYPE_SOFTWARE_DEVICE: u32 = 1 << 2;
const ADL_OK: i32 = 0;
const ADL_PMLOG_TEMPERATURE_EDGE: usize = 8;
const ADL_PMLOG_TEMPERATURE_HOTSPOT: usize = 27;
const ADL_PMLOG_TEMPERATURE_GFX: usize = 28;
const ADL_PMLOG_TEMPERATURE_SOC: usize = 29;
const ADL_PMLOG_TEMPERATURE_CPU: usize = 32;
const ADL_PMLOG_MAX_SENSORS: usize = 256;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TemperatureSource {
    Gpu,
    Cpu,
    Soc,
    Hotspot,
    Ec,
}

impl TemperatureSource {
    pub const ALL: [TemperatureSource; 5] = [
        TemperatureSource::Gpu,
        TemperatureSource::Cpu,
        TemperatureSource::Soc,
        TemperatureSource::Hotspot,
        TemperatureSource::Ec,
    ];

    pub fn from_code(code: &str) -> Self {
        match code {
            "cpu" => TemperatureSource::Cpu,
            "soc" => TemperatureSource::Soc,
            "hotspot" => TemperatureSource::Hotspot,
            "ec" => TemperatureSource::Ec,
            _ => TemperatureSource::Gpu,
        }
    }

    pub fn code(self) -> &'static str {
        match self {
            TemperatureSource::Gpu => "gpu",
            TemperatureSource::Cpu => "cpu",
            TemperatureSource::Soc => "soc",
            TemperatureSource::Hotspot => "hotspot",
            TemperatureSource::Ec => "ec",
        }
    }

    pub fn i18n_key(self) -> &'static str {
        match self {
            TemperatureSource::Gpu => "temp_src_gpu",
            TemperatureSource::Cpu => "temp_src_cpu",
            TemperatureSource::Soc => "temp_src_soc",
            TemperatureSource::Hotspot => "temp_src_hotspot",
            TemperatureSource::Ec => "temp_src_ec",
        }
    }

    pub fn as_log_str(self) -> &'static str {
        match self {
            TemperatureSource::Gpu => "GPU (Task Manager / ADL)",
            TemperatureSource::Cpu => "CPU (AMD ADL)",
            TemperatureSource::Soc => "SoC (AMD ADL)",
            TemperatureSource::Hotspot => "GPU hotspot (AMD ADL)",
            TemperatureSource::Ec => "EC 0x70",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ThermalSnapshot {
    pub gpu: Option<u8>,
    pub cpu: Option<u8>,
    pub soc: Option<u8>,
    pub hotspot: Option<u8>,
    pub ec_raw: u8,
    pub control: u8,
    pub control_source: TemperatureSource,
}

pub fn resolve_temperature(ec_celsius: u8, preferred: TemperatureSource) -> u8 {
    read_thermal(ec_celsius, preferred).control
}

pub fn read_thermal(ec_celsius: u8, preferred: TemperatureSource) -> ThermalSnapshot {
    let adl = read_adl_sensors();
    let gpu = read_wddm_gpu_temp().or(adl.gpu).or_else(read_adl_od5_gpu);
    let (control, control_source) =
        pick_control_temp(preferred, gpu, adl.cpu, adl.soc, adl.hotspot, ec_celsius);
    ThermalSnapshot {
        gpu,
        cpu: adl.cpu,
        soc: adl.soc,
        hotspot: adl.hotspot,
        ec_raw: ec_celsius,
        control,
        control_source,
    }
}

pub fn pick_control_temp(
    preferred: TemperatureSource,
    gpu: Option<u8>,
    cpu: Option<u8>,
    soc: Option<u8>,
    hotspot: Option<u8>,
    ec_celsius: u8,
) -> (u8, TemperatureSource) {
    let selected = match preferred {
        TemperatureSource::Gpu => gpu.map(|temp| (temp, TemperatureSource::Gpu)),
        TemperatureSource::Cpu => cpu.map(|temp| (temp, TemperatureSource::Cpu)),
        TemperatureSource::Soc => soc.map(|temp| (temp, TemperatureSource::Soc)),
        TemperatureSource::Hotspot => hotspot.map(|temp| (temp, TemperatureSource::Hotspot)),
        TemperatureSource::Ec => Some((ec_celsius, TemperatureSource::Ec)),
    };
    if let Some(pair) = selected {
        return pair;
    }
    if let Some(temp) = gpu {
        return (temp, TemperatureSource::Gpu);
    }
    (ec_celsius, TemperatureSource::Ec)
}

pub fn describe_source(ec_celsius: u8, preferred: TemperatureSource) -> String {
    let snap = read_thermal(ec_celsius, preferred);
    format!(
        "Control {control}°C from {source} (preferred={preferred}, GPU={gpu}, CPU={cpu}, SoC={soc}, hotspot={hotspot}, EC 0x70={ec}°C)",
        control = snap.control,
        source = snap.control_source.as_log_str(),
        preferred = preferred.as_log_str(),
        gpu = fmt_opt(snap.gpu),
        cpu = fmt_opt(snap.cpu),
        soc = fmt_opt(snap.soc),
        hotspot = fmt_opt(snap.hotspot),
        ec = snap.ec_raw
    )
}

fn fmt_opt(value: Option<u8>) -> String {
    value
        .map(|temp| format!("{temp}°C"))
        .unwrap_or_else(|| "n/a".into())
}

#[derive(Clone, Copy, Default)]
struct AdlSensors {
    gpu: Option<u8>,
    cpu: Option<u8>,
    soc: Option<u8>,
    hotspot: Option<u8>,
}

fn plausible_celsius(value: u32) -> Option<u8> {
    (10..=115).contains(&value).then_some(value as u8)
}

fn deci_celsius_to_u8(deci: u32) -> Option<u8> {
    if deci == 0 {
        return None;
    }
    plausible_celsius((deci + 5) / 10)
}

fn millicelsius_to_u8(milli: i32) -> Option<u8> {
    if milli <= 0 {
        return None;
    }
    let rounded = (milli + 500) / 1000;
    if rounded < 0 {
        return None;
    }
    plausible_celsius(rounded as u32)
}

#[repr(C)]
#[derive(Clone, Copy)]
struct D3dkmtAdapterInfo {
    h_adapter: u32,
    adapter_luid_low: u32,
    adapter_luid_high: i32,
    num_of_sources: u32,
    present_move_regions_preferred: i32,
}

#[repr(C)]
struct D3dkmtEnumAdapters2 {
    num_adapters: u32,
    p_adapters: *mut D3dkmtAdapterInfo,
}

#[repr(C)]
struct D3dkmtQueryAdapterInfo {
    h_adapter: u32,
    type_: i32,
    p_private_driver_data: *mut c_void,
    private_driver_data_size: u32,
}

#[repr(C)]
struct D3dkmtCloseAdapter {
    h_adapter: u32,
}

#[repr(C)]
struct D3dkmtAdapterType {
    value: u32,
}

#[repr(C)]
struct D3dkmtQueryDeviceIds {
    physical_adapter_index: u32,
    vendor_id: u32,
    device_id: u32,
    sub_vendor_id: u32,
    sub_system_id: u32,
    revision_id: u32,
    bus_type: u32,
}

#[repr(C)]
struct D3dkmtAdapterPerfData {
    physical_adapter_index: u32,
    memory_frequency: u64,
    max_memory_frequency: u64,
    max_memory_frequency_oc: u64,
    memory_bandwidth: u64,
    pcie_bandwidth: u64,
    fan_rpm: u32,
    power: u32,
    temperature: u32,
    power_state_override: u8,
}

type FnEnumAdapters2 = unsafe extern "system" fn(*mut D3dkmtEnumAdapters2) -> i32;
type FnQueryAdapterInfo = unsafe extern "system" fn(*const D3dkmtQueryAdapterInfo) -> i32;
type FnCloseAdapter = unsafe extern "system" fn(*const D3dkmtCloseAdapter) -> i32;

struct WddmApi {
    enum_adapters2: FnEnumAdapters2,
    query: FnQueryAdapterInfo,
    close: FnCloseAdapter,
}

fn wddm_api() -> Option<&'static WddmApi> {
    static API: OnceLock<Option<WddmApi>> = OnceLock::new();
    API.get_or_init(load_wddm_api).as_ref()
}

fn load_wddm_api() -> Option<WddmApi> {
    unsafe {
        let module = load_system32_library("gdi32.dll");
        if module.is_null() {
            return None;
        }
        Some(WddmApi {
            enum_adapters2: load_fn(module, b"D3DKMTEnumAdapters2\0")?,
            query: load_fn(module, b"D3DKMTQueryAdapterInfo\0")?,
            close: load_fn(module, b"D3DKMTCloseAdapter\0")?,
        })
    }
}

fn read_wddm_gpu_temp() -> Option<u8> {
    let api = wddm_api()?;
    let mut adapters = [MaybeUninit::<D3dkmtAdapterInfo>::uninit(); 16];
    let mut enum_adapters = D3dkmtEnumAdapters2 {
        num_adapters: adapters.len() as u32,
        p_adapters: adapters.as_mut_ptr() as *mut D3dkmtAdapterInfo,
    };

    if unsafe { (api.enum_adapters2)(&mut enum_adapters) } < 0 {
        return None;
    }

    let count = (enum_adapters.num_adapters as usize).min(adapters.len());
    let mut amd_temp = None;
    let mut other_temp = None;

    for adapter in adapters.iter().take(count) {
        let adapter = unsafe { adapter.assume_init() };
        let reading = unsafe { read_wddm_adapter(api, adapter.h_adapter) };
        unsafe {
            (api.close)(&D3dkmtCloseAdapter {
                h_adapter: adapter.h_adapter,
            });
        }
        let Some((temp, vendor)) = reading else {
            continue;
        };
        if vendor == Some(AMD_VENDOR_ID) {
            amd_temp = Some(temp);
            break;
        }
        if other_temp.is_none() {
            other_temp = Some(temp);
        }
    }

    amd_temp.or(other_temp)
}

unsafe fn read_wddm_adapter(api: &WddmApi, handle: u32) -> Option<(u8, Option<u32>)> {
    let mut adapter_type = D3dkmtAdapterType { value: 0 };
    if !query_adapter(api, handle, KMTQAITYPE_ADAPTERTYPE, &mut adapter_type) {
        return None;
    }
    if adapter_type.value & ADAPTER_TYPE_SOFTWARE_DEVICE != 0 {
        return None;
    }

    let mut ids = D3dkmtQueryDeviceIds {
        physical_adapter_index: 0,
        vendor_id: 0,
        device_id: 0,
        sub_vendor_id: 0,
        sub_system_id: 0,
        revision_id: 0,
        bus_type: 0,
    };
    let vendor = if query_adapter(api, handle, KMTQAITYPE_PHYSICALADAPTERDEVICEIDS, &mut ids) {
        Some(ids.vendor_id)
    } else {
        None
    };

    let mut perf = D3dkmtAdapterPerfData {
        physical_adapter_index: 0,
        memory_frequency: 0,
        max_memory_frequency: 0,
        max_memory_frequency_oc: 0,
        memory_bandwidth: 0,
        pcie_bandwidth: 0,
        fan_rpm: 0,
        power: 0,
        temperature: 0,
        power_state_override: 0,
    };
    if !query_adapter(api, handle, KMTQAITYPE_ADAPTERPERFDATA, &mut perf) {
        return None;
    }
    Some((deci_celsius_to_u8(perf.temperature)?, vendor))
}

unsafe fn query_adapter<T>(api: &WddmApi, handle: u32, info_type: i32, data: &mut T) -> bool {
    let info = D3dkmtQueryAdapterInfo {
        h_adapter: handle,
        type_: info_type,
        p_private_driver_data: data as *mut T as *mut c_void,
        private_driver_data_size: size_of::<T>() as u32,
    };
    (api.query)(&info) >= 0
}

type AdlMallocCb = unsafe extern "system" fn(i32) -> *mut c_void;
type FnAdlCreate = unsafe extern "system" fn(AdlMallocCb, i32, *mut *mut c_void) -> i32;
type FnAdlDestroy = unsafe extern "system" fn(*mut c_void) -> i32;
type FnAdlAdapterCount = unsafe extern "system" fn(*mut c_void, *mut i32) -> i32;
type FnAdlOd5Temp = unsafe extern "system" fn(*mut c_void, i32, i32, *mut AdlTemperature) -> i32;
type FnAdlPmLog = unsafe extern "system" fn(*mut c_void, i32, *mut AdlPmLogDataOutput) -> i32;

#[repr(C)]
struct AdlTemperature {
    size: i32,
    temperature: i32,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct AdlSingleSensorData {
    supported: i32,
    value: i32,
}

#[repr(C)]
struct AdlPmLogDataOutput {
    size: i32,
    sensors: [AdlSingleSensorData; ADL_PMLOG_MAX_SENSORS],
}

struct AdlApi {
    context: *mut c_void,
    destroy: FnAdlDestroy,
    adapter_count: FnAdlAdapterCount,
    od5_temp: Option<FnAdlOd5Temp>,
    pmlog: Option<FnAdlPmLog>,
}

unsafe impl Send for AdlApi {}
unsafe impl Sync for AdlApi {}

impl Drop for AdlApi {
    fn drop(&mut self) {
        unsafe {
            (self.destroy)(self.context);
        }
    }
}

fn adl_api() -> Option<&'static Mutex<AdlApi>> {
    static API: OnceLock<Option<Mutex<AdlApi>>> = OnceLock::new();
    API.get_or_init(load_adl_api).as_ref()
}

fn load_adl_api() -> Option<Mutex<AdlApi>> {
    unsafe {
        let module = load_system32_library("atiadlxx.dll");
        if module.is_null() {
            return None;
        }
        let create: FnAdlCreate = load_fn(module, b"ADL2_Main_Control_Create\0")?;
        let destroy: FnAdlDestroy = load_fn(module, b"ADL2_Main_Control_Destroy\0")?;
        let adapter_count: FnAdlAdapterCount =
            load_fn(module, b"ADL2_Adapter_NumberOfAdapters_Get\0")?;
        let od5_temp = load_fn(module, b"ADL2_Overdrive5_Temperature_Get\0");
        let pmlog = load_fn(module, b"ADL2_New_QueryPMLogData_Get\0");
        if od5_temp.is_none() && pmlog.is_none() {
            return None;
        }

        let mut context = ptr::null_mut();
        if create(adl_malloc, 1, &mut context) != ADL_OK || context.is_null() {
            return None;
        }

        Some(Mutex::new(AdlApi {
            context,
            destroy,
            adapter_count,
            od5_temp,
            pmlog,
        }))
    }
}

extern "C" {
    fn malloc(size: usize) -> *mut c_void;
}

unsafe extern "system" fn adl_malloc(size: i32) -> *mut c_void {
    if size <= 0 {
        return ptr::null_mut();
    }
    malloc(size as usize)
}

fn read_adl_sensors() -> AdlSensors {
    let Some(api) = adl_api() else {
        return AdlSensors::default();
    };
    let Ok(api) = api.lock() else {
        return AdlSensors::default();
    };
    unsafe { query_adl_sensors(&api) }
}

fn read_adl_od5_gpu() -> Option<u8> {
    let api = adl_api()?;
    let api = api.lock().ok()?;
    unsafe { query_adl_od5(&api) }
}

unsafe fn query_adl_od5(api: &AdlApi) -> Option<u8> {
    let od5 = api.od5_temp?;
    let mut num = 0i32;
    if (api.adapter_count)(api.context, &mut num) != ADL_OK || num <= 0 {
        return None;
    }
    for index in 0..num {
        let mut temp = AdlTemperature {
            size: size_of::<AdlTemperature>() as i32,
            temperature: 0,
        };
        if od5(api.context, index, 0, &mut temp) == ADL_OK {
            if let Some(celsius) = millicelsius_to_u8(temp.temperature) {
                return Some(celsius);
            }
        }
    }
    None
}

unsafe fn query_adl_sensors(api: &AdlApi) -> AdlSensors {
    let mut sensors = AdlSensors::default();
    let Some(pmlog) = api.pmlog else {
        return sensors;
    };
    let mut num = 0i32;
    if (api.adapter_count)(api.context, &mut num) != ADL_OK || num <= 0 {
        return sensors;
    }

    for index in 0..num {
        let mut output = AdlPmLogDataOutput {
            size: size_of::<AdlPmLogDataOutput>() as i32,
            sensors: [AdlSingleSensorData {
                supported: 0,
                value: 0,
            }; ADL_PMLOG_MAX_SENSORS],
        };
        if pmlog(api.context, index, &mut output) != ADL_OK {
            continue;
        }
        sensors.gpu = sensors
            .gpu
            .or_else(|| pmlog_sensor(&output, ADL_PMLOG_TEMPERATURE_GFX))
            .or_else(|| pmlog_sensor(&output, ADL_PMLOG_TEMPERATURE_EDGE));
        sensors.cpu = sensors
            .cpu
            .or_else(|| pmlog_sensor(&output, ADL_PMLOG_TEMPERATURE_CPU));
        sensors.soc = sensors
            .soc
            .or_else(|| pmlog_sensor(&output, ADL_PMLOG_TEMPERATURE_SOC));
        sensors.hotspot = sensors
            .hotspot
            .or_else(|| pmlog_sensor(&output, ADL_PMLOG_TEMPERATURE_HOTSPOT));
        if sensors.gpu.is_some()
            && sensors.cpu.is_some()
            && sensors.soc.is_some()
            && sensors.hotspot.is_some()
        {
            break;
        }
    }
    sensors
}

fn pmlog_sensor(output: &AdlPmLogDataOutput, index: usize) -> Option<u8> {
    let sensor = output.sensors.get(index)?;
    if sensor.supported == 0 || sensor.value < 0 {
        return None;
    }
    plausible_celsius(sensor.value as u32)
}

unsafe fn load_fn<T: Copy>(module: HMODULE, name: &[u8]) -> Option<T> {
    let symbol = GetProcAddress(module, name.as_ptr() as *const i8);
    if symbol.is_null() {
        return None;
    }
    Some(std::mem::transmute_copy::<_, T>(&symbol))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wddm_perfdata_layout_matches_ddi() {
        assert_eq!(size_of::<D3dkmtAdapterPerfData>(), 64);
        assert_eq!(std::mem::offset_of!(D3dkmtAdapterPerfData, temperature), 56);
        assert_eq!(size_of::<D3dkmtEnumAdapters2>(), 16);
        assert_eq!(size_of::<D3dkmtAdapterInfo>(), 20);
    }

    #[test]
    fn converts_task_manager_style_deci_celsius() {
        assert_eq!(deci_celsius_to_u8(450), Some(45));
        assert_eq!(deci_celsius_to_u8(454), Some(45));
        assert_eq!(deci_celsius_to_u8(455), Some(46));
        assert_eq!(deci_celsius_to_u8(0), None);
        assert_eq!(deci_celsius_to_u8(2000), None);
    }

    #[test]
    fn converts_adl_millicelsius() {
        assert_eq!(millicelsius_to_u8(45_000), Some(45));
        assert_eq!(millicelsius_to_u8(0), None);
        assert_eq!(millicelsius_to_u8(-1), None);
    }

    #[test]
    fn pick_control_uses_preferred_sensor() {
        assert_eq!(
            pick_control_temp(
                TemperatureSource::Gpu,
                Some(48),
                Some(62),
                Some(51),
                Some(80),
                97
            ),
            (48, TemperatureSource::Gpu)
        );
        assert_eq!(
            pick_control_temp(
                TemperatureSource::Cpu,
                Some(48),
                Some(62),
                Some(51),
                Some(80),
                97
            ),
            (62, TemperatureSource::Cpu)
        );
        assert_eq!(
            pick_control_temp(
                TemperatureSource::Hotspot,
                Some(48),
                Some(62),
                Some(51),
                Some(80),
                97
            ),
            (80, TemperatureSource::Hotspot)
        );
        assert_eq!(
            pick_control_temp(
                TemperatureSource::Ec,
                Some(48),
                Some(62),
                Some(51),
                Some(80),
                97
            ),
            (97, TemperatureSource::Ec)
        );
    }

    #[test]
    fn pick_control_falls_back_to_gpu_then_ec() {
        assert_eq!(
            pick_control_temp(
                TemperatureSource::Cpu,
                Some(55),
                None,
                Some(51),
                Some(80),
                97
            ),
            (55, TemperatureSource::Gpu)
        );
        assert_eq!(
            pick_control_temp(
                TemperatureSource::Gpu,
                None,
                Some(62),
                Some(51),
                Some(80),
                42
            ),
            (42, TemperatureSource::Ec)
        );
        assert_eq!(
            pick_control_temp(TemperatureSource::Soc, None, None, None, None, 42),
            (42, TemperatureSource::Ec)
        );
    }

    #[test]
    fn temperature_source_defaults_unknown_to_gpu() {
        assert_eq!(TemperatureSource::from_code("gpu"), TemperatureSource::Gpu);
        assert_eq!(TemperatureSource::from_code(""), TemperatureSource::Gpu);
        assert_eq!(TemperatureSource::from_code("nope"), TemperatureSource::Gpu);
        assert_eq!(
            TemperatureSource::from_code("hotspot"),
            TemperatureSource::Hotspot
        );
        assert_eq!(TemperatureSource::from_code("ec").code(), "ec");
    }

    #[test]
    fn ec_fallback_used_when_gpu_apis_return_nothing() {
        let snap = read_thermal(42, TemperatureSource::Gpu);
        if snap.control_source == TemperatureSource::Ec {
            assert_eq!(snap.control, 42);
            assert_eq!(snap.ec_raw, 42);
        } else {
            assert!((10..=115).contains(&snap.control));
        }
    }
}
