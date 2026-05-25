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

#[cfg(test)]
mod tests {
    use super::*;
    use uncode_core::message::{ContentBlock, Role};

    #[test]
    fn test_step_count_not_reached() {
        let sc = step_count_is(5);
        assert!(sc.should_stop(3, &[]).is_none());
    }

    #[test]
    fn test_step_count_exactly_reached() {
        let sc = step_count_is(5);
        assert!(matches!(sc.should_stop(5, &[]), Some(StopReason::MaxSteps)));
    }

    #[test]
    fn test_step_count_exceeded() {
        let sc = step_count_is(5);
        assert!(matches!(sc.should_stop(6, &[]), Some(StopReason::MaxSteps)));
    }

    #[test]
    fn test_text_contains_match() {
        let msg = Message::user("error occurred somewhere");
        let tc = text_contains("error");
        assert!(matches!(
            tc.should_stop(0, &[msg]),
            Some(StopReason::Completed)
        ));
    }

    #[test]
    fn test_text_contains_no_match() {
        let msg = Message::user("everything is fine");
        let tc = text_contains("error");
        assert!(tc.should_stop(0, &[msg]).is_none());
    }

    #[test]
    fn test_text_contains_only_checks_text_blocks() {
        let msg = Message::new(
            Role::User,
            vec![ContentBlock::Thinking {
                text: "error in reasoning".into(),
            }],
        );
        let tc = text_contains("error");
        assert!(tc.should_stop(0, &[msg]).is_none());
    }

    #[test]
    fn test_step_count_is_constructor() {
        let sc = step_count_is(10);
        assert!(sc.should_stop(9, &[]).is_none());
        assert!(matches!(
            sc.should_stop(10, &[]),
            Some(StopReason::MaxSteps)
        ));
    }
}
