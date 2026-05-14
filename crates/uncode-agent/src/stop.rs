use uncode_core::message::Message;

pub enum StopReason {
    MaxSteps,
    Predicate,
    Completed,
}

pub trait StopCondition: Send + Sync {
    fn should_stop(&self, turn: u64, messages: &[Message]) -> Option<StopReason>;
}

pub struct StepCountStop {
    max: u64,
}

impl StopCondition for StepCountStop {
    fn should_stop(&self, turn: u64, _messages: &[Message]) -> Option<StopReason> {
        if turn >= self.max {
            Some(StopReason::MaxSteps)
        } else {
            None
        }
    }
}

pub fn step_count_is(max: u64) -> Box<dyn StopCondition> {
    Box::new(StepCountStop { max })
}

pub struct TextContainsStop {
    text: String,
}

impl StopCondition for TextContainsStop {
    fn should_stop(&self, _turn: u64, messages: &[Message]) -> Option<StopReason> {
        for msg in messages.iter().rev().take(3) {
            for block in &msg.content {
                if let uncode_core::message::ContentBlock::Text { text } = block {
                    if text.contains(&self.text) {
                        return Some(StopReason::Completed);
                    }
                }
            }
        }
        None
    }
}

pub fn text_contains(pattern: impl Into<String>) -> Box<dyn StopCondition> {
    Box::new(TextContainsStop {
        text: pattern.into(),
    })
}
