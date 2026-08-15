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
}
