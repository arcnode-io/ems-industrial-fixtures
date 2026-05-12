//! Sawtooth simulator. Ticks every second, mutates the in-memory Analog
//! Input present_values that the responder reads when answering ReadProperty.

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

/// One per Analog Input object. Sawtooth between `min` and `max`, stepping by
/// `step` each tick.
#[derive(Clone, Copy)]
pub struct SawStrategy {
    /// Inclusive lower bound of the ramp.
    pub min: f32,
    /// Inclusive upper bound of the ramp.
    pub max: f32,
    /// Per-tick increment.
    pub step: f32,
}

/// Shared `object_instance -> present_value` map, mutated by the simulator
/// and read by the responder loop.
pub type Values = Arc<Mutex<HashMap<u32, f32>>>;

/// Build the initial value map at the low end of each strategy's range.
pub fn seed(strategies: &HashMap<u32, SawStrategy>) -> Values {
    let initial: HashMap<u32, f32> = strategies.iter().map(|(idx, s)| (*idx, s.min)).collect();
    Arc::new(Mutex::new(initial))
}

/// Step every tracked object by its strategy's `step`; wrap to `min` on
/// overshoot.
pub async fn tick(values: &Values, strategies: &HashMap<u32, SawStrategy>) {
    let mut guard = values.lock().await;
    for (idx, s) in strategies {
        let cur = guard.get(idx).copied().unwrap_or(s.min);
        let next = if cur + s.step > s.max {
            s.min
        } else {
            cur + s.step
        };
        guard.insert(*idx, next);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strat() -> HashMap<u32, SawStrategy> {
        let mut m = HashMap::new();
        m.insert(
            1,
            SawStrategy {
                min: 7.0,
                max: 15.0,
                step: 0.5,
            },
        );
        m
    }

    #[tokio::test]
    async fn tick_increments_within_range() {
        // Arrange
        let strategies = strat();
        let values = seed(&strategies);
        // Act
        tick(&values, &strategies).await;
        // Assert
        let actual = *values.lock().await.get(&1).unwrap();
        let expected = 7.5;
        assert_eq!(actual, expected);
    }

    #[tokio::test]
    async fn tick_wraps_at_max() {
        // Arrange
        let strategies = strat();
        let values = seed(&strategies);
        values.lock().await.insert(1, 14.8); // 14.8 + 0.5 > 15.0
        // Act
        tick(&values, &strategies).await;
        // Assert
        let actual = *values.lock().await.get(&1).unwrap();
        let expected = 7.0;
        assert_eq!(actual, expected);
    }
}
