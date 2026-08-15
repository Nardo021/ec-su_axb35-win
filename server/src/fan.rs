pub const NAME_MAX_CHARS: usize = 24;

pub fn parse_fan_id(spec: &str) -> Result<u8, String> {
    match spec.trim().to_ascii_lowercase().as_str() {
        "1" | "cpu" => Ok(1),
        "2" | "secondary" => Ok(2),
        "3" | "system" => Ok(3),
        _ => Err(format!(
            "Unknown fan '{spec}'. Use 1|2|3 or cpu|secondary|system."
        )),
    }
}

pub fn title_key(fan_id: u8) -> &'static str {
    match fan_id {
        1 => "fan_cpu",
        2 => "fan_secondary",
        _ => "fan_system",
    }
}

pub fn cli_label_key(fan_id: u8) -> &'static str {
    match fan_id {
        1 => "cpu_fan",
        2 => "secondary_fan",
        _ => "system_fan",
    }
}

pub fn alias(fan_id: u8) -> &'static str {
    match fan_id {
        1 => "cpu",
        2 => "secondary",
        _ => "system",
    }
}

pub fn parse_curve(spec: &str) -> Result<[u8; 5], String> {
    let parts: Vec<&str> = spec
        .split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .collect();
    if parts.len() != 5 {
        return Err("Fan curve must have exactly 5 comma-separated values".to_string());
    }
    let mut curve = [0u8; 5];
    for (index, part) in parts.iter().enumerate() {
        curve[index] = part
            .parse()
            .map_err(|_| format!("Invalid fan curve value: {part}"))?;
    }
    Ok(curve)
}

pub fn sanitize_name(name: &str) -> Option<String> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return None;
    }
    let mut cleaned = String::new();
    for ch in trimmed.chars() {
        if ch == '\n' || ch == '\r' {
            continue;
        }
        cleaned.push(ch);
        if cleaned.chars().count() >= NAME_MAX_CHARS {
            break;
        }
    }
    let cleaned = cleaned.trim();
    if cleaned.is_empty() {
        None
    } else {
        Some(cleaned.to_string())
    }
}

pub fn display_name(custom: Option<&str>, default: &str) -> String {
    custom
        .and_then(sanitize_name)
        .unwrap_or_else(|| default.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fan_id_aliases() {
        assert_eq!(parse_fan_id("1").unwrap(), 1);
        assert_eq!(parse_fan_id("CPU").unwrap(), 1);
        assert_eq!(parse_fan_id("secondary").unwrap(), 2);
        assert_eq!(parse_fan_id("3").unwrap(), 3);
        assert_eq!(parse_fan_id("system").unwrap(), 3);
        assert!(parse_fan_id("4").is_err());
        assert!(parse_fan_id("gpu").is_err());
    }

    #[test]
    fn curve_requires_five_values() {
        assert_eq!(parse_curve("20,60,83,95,97").unwrap(), [20, 60, 83, 95, 97]);
        assert!(parse_curve("20,60,83").is_err());
        assert!(parse_curve("a,b,c,d,e").is_err());
    }

    #[test]
    fn sanitize_name_treats_blank_as_default() {
        assert_eq!(sanitize_name(""), None);
        assert_eq!(sanitize_name("   "), None);
        assert_eq!(sanitize_name("\n\t"), None);
    }

    #[test]
    fn sanitize_name_trims_and_caps_length() {
        assert_eq!(sanitize_name("  Exhaust  ").as_deref(), Some("Exhaust"));
        let long = "abcdefghij".repeat(5);
        let cleaned = sanitize_name(&long).unwrap();
        assert_eq!(cleaned.chars().count(), NAME_MAX_CHARS);
        assert_eq!(sanitize_name("Rear\nintake").as_deref(), Some("Rearintake"));
    }

    #[test]
    fn custom_name_overrides_default_label() {
        assert_eq!(display_name(Some("Chassis"), "System Fan"), "Chassis");
        assert_eq!(display_name(Some("  "), "System Fan"), "System Fan");
        assert_eq!(display_name(None, "CPU Fan"), "CPU Fan");
    }
}
