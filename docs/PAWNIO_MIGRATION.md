# PawnIO migration

This fork replaces WinRing0 with official PawnIO so the EVO-X2 / Sixunited
AXB35 Embedded Controller can be controlled on Windows 11 with Secure Boot
enabled.

## Old architecture

```text
Application
    ↓
WinRing0 (WinRing0x64.sys)
    ↓
Kernel
    ↓
arbitrary I/O ports
    ↓
EVO-X2 EC
```

WinRing0 is an unsigned / test-signed style kernel driver. Loading it on
modern Windows 11 typically requires disabling Secure Boot, enabling Test
Signing, or weakening driver signature enforcement. Those approaches are
intentionally out of scope for this fork.

## New architecture

```text
evox2-control.exe  (window + tray, or evox2ctl)
    ↓
PawnIO userspace API (CreateFile + DeviceIoControl)
    ↓
signed official PawnIO driver
    ↓
official LpcACPIEC module
    ↓
only 0x62 / 0x66
    ↓
EVO-X2 EC
```

The EC protocol itself is unchanged. Only the port-access primitive changed:

```text
WinRing0 ReadIoPort/WriteIoPort
        ↓
PawnIO LpcACPIEC ioctl_pio_read / ioctl_pio_write
```

## Why WinRing0 was removed

- It cannot load under a default Secure Boot + HVCI Windows 11 install.
- It exposes general-purpose I/O port access, which is far broader than this
  application needs.
- Microsoft and anti-cheat vendors treat WinRing0-class drivers as vulnerable
  or blocked.

## PawnIO integration

Userspace talks to the official driver using the documented IOCTL interface
from `PawnIO/include/pawnio_um.h` and `PawnIOLib`:

| Item | Official value |
| --- | --- |
| Device | `\Device\PawnIO` opened as `\\?\GLOBALROOT\Device\PawnIO` |
| Device type | `41394` |
| Load module | `IOCTL_PIO_LOAD_BINARY` (`CTL_CODE(41394, 0x821, METHOD_BUFFERED, FILE_ANY_ACCESS)`) |
| Execute | `IOCTL_PIO_EXECUTE_FN` (`CTL_CODE(41394, 0x841, METHOD_BUFFERED, FILE_ANY_ACCESS)`) |
| Version | `IOCTL_PIO_VERSION` (`CTL_CODE(41394, 0x861, METHOD_BUFFERED, FILE_ANY_ACCESS)`) |

`LpcACPIEC` functions:

| Function | Input cells | Output cells |
| --- | --- | --- |
| `ioctl_pio_read` | `[port]` | `[value]` |
| `ioctl_pio_write` | `[port, value]` | none |

The official signed `LpcACPIEC.bin` from PawnIO.Modules 0.2.10 is embedded at
build time. See `server/vendor/pawnio/README.md`.

The application does **not** install PawnIO itself and never downloads a kernel
binary at runtime. Install the official signed PawnIO release from
https://pawnio.eu/. If PawnIO is missing, the current-language message box
offers to open that page.

## EC port restrictions

`LpcACPIEC` only allows ports `0x62` (data) and `0x66` (command/status). The
application also refuses any other port before calling PawnIO.

Complete EC transactions (command + address + data) are serialized with:

1. An in-process mutex
2. The documented `Global\Access_EC` mutant (`\BaseNamedObjects\Access_EC`)

## Security implications

- Secure Boot, Test Signing, and Memory Integrity can remain in their secure
  defaults.
- There is no REST API and no Windows service.
- There is no arbitrary EC write endpoint.
- Unknown machines can be observed but will not receive speculative EC writes.

## Remaining limitations

- The single-window PawnIO path has been validated on a GMKtec EVO-X2.
- Official PawnIO must already be installed.
- The official PawnIO driver will reject unofficial / unsigned modules.
- Some anti-cheat software may still object to any hardware-access driver,
  including signed ones.
- Firmware older than 1.04 remains unsupported, matching the upstream README.
