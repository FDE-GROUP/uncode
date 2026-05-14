use uncode_core::message::{ContentBlock, Message};

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

/// 创建步数限制的 StopCondition
pub fn step_count_is(max: u64) -> Box<dyn StopCondition> {
    Box::new(StepCountStop { max })
}

pub struct TextContainsStop {
    text: String,
}

impl StopCondition for TextContainsStop {
    fn should_stop(&self, _turn: u64, messages: &[Message]) -> Option<StopReason> {
        let found = messages
            .iter()
            .rev()
            .take(3)
            .flat_map(|msg| msg.content.iter())
            .any(|block| matches!(block, ContentBlock::Text { text } if text.contains(&self.text)));
        found.then_some(StopReason::Completed)
    }
}

/// 创建文本匹配的 StopCondition（检测到指定文本时停止）
pub fn text_contains(pattern: impl Into<String>) -> Box<dyn StopCondition> {
    Box::new(TextContainsStop {
        text: pattern.into(),
    })
}
