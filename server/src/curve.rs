use std::collections::VecDeque;

pub fn average_temps(samples: &[u8]) -> Option<u8> {
    if samples.is_empty() {
        return None;
    }
    let sum: u32 = samples.iter().map(|&sample| u32::from(sample)).sum();
    let count = samples.len() as u32;
    Some(((sum + count / 2) / count) as u8)
}

pub fn next_fan_level(current: u8, temp: u8, rampup: &[u8; 5], rampdown: &[u8; 5]) -> u8 {
    let current = current.min(5);
    if current < 5 && temp >= rampup[current as usize] {
        current + 1
    } else if current > 0 && temp <= rampdown[(current - 1) as usize] {
        current - 1
    } else {
        current
    }
}

pub fn push_temp_sample(window: &mut VecDeque<u8>, temp: u8, max_len: u8) {
    let max_len = usize::from(max_len.max(1));
    window.push_back(temp);
    while window.len() > max_len {
        window.pop_front();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn average_temps_empty_is_none() {
        assert_eq!(average_temps(&[]), None);
    }

    #[test]
    fn average_temps_rounds_half_up() {
        assert_eq!(average_temps(&[40]), Some(40));
        assert_eq!(average_temps(&[59, 60]), Some(60));
        assert_eq!(average_temps(&[50, 50, 50, 70]), Some(55));
    }

    #[test]
    fn next_fan_level_ramps_up_then_down() {
        let rampup = [60, 70, 83, 95, 97];
        let rampdown = [40, 50, 80, 94, 96];
        assert_eq!(next_fan_level(0, 59, &rampup, &rampdown), 0);
        assert_eq!(next_fan_level(0, 60, &rampup, &rampdown), 1);
        assert_eq!(next_fan_level(1, 69, &rampup, &rampdown), 1);
        assert_eq!(next_fan_level(1, 40, &rampup, &rampdown), 0);
        assert_eq!(next_fan_level(5, 96, &rampup, &rampdown), 4);
    }

    #[test]
    fn push_temp_sample_drops_oldest() {
        let mut window = VecDeque::new();
        for temp in [10, 20, 30, 40] {
            push_temp_sample(&mut window, temp, 3);
        }
        assert_eq!(window.iter().copied().collect::<Vec<_>>(), vec![20, 30, 40]);
        push_temp_sample(&mut window, 50, 1);
        assert_eq!(window.iter().copied().collect::<Vec<_>>(), vec![50]);
    }
}
