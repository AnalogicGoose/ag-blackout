use std::collections::BTreeSet;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum CoreLesson {
    Terminal,
    RemoteWork,
    Investigation,
}

pub const CORE_LESSONS: [CoreLesson; 3] = [
    CoreLesson::Terminal,
    CoreLesson::RemoteWork,
    CoreLesson::Investigation,
];

impl CoreLesson {
    pub const fn id(self) -> &'static str {
        match self {
            Self::Terminal => "terminal",
            Self::RemoteWork => "remote-work",
            Self::Investigation => "investigation",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        match id {
            "terminal" => Some(Self::Terminal),
            "remote-work" => Some(Self::RemoteWork),
            "investigation" => Some(Self::Investigation),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum Capability {
    Privilege,
    ServiceFoothold,
}

impl Capability {
    pub const fn id(self) -> &'static str {
        match self {
            Self::Privilege => "privilege",
            Self::ServiceFoothold => "service-foothold",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        match id {
            "privilege" => Some(Self::Privilege),
            "service-foothold" => Some(Self::ServiceFoothold),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LessonId {
    Core(CoreLesson),
    Capability(Capability),
}

#[derive(Default)]
pub struct TutorialProgress {
    completed_core: BTreeSet<CoreLesson>,
    unlocked_capabilities: BTreeSet<Capability>,
    offered_capabilities: BTreeSet<Capability>,
    completed_capability_lessons: BTreeSet<Capability>,
}

impl TutorialProgress {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn current_core(&self) -> Option<CoreLesson> {
        CORE_LESSONS
            .iter()
            .copied()
            .find(|lesson| !self.completed_core.contains(lesson))
    }

    pub fn core_available(&self, lesson: CoreLesson) -> bool {
        self.completed_core.contains(&lesson) || self.current_core() == Some(lesson)
    }

    pub fn complete_core(&mut self, lesson: CoreLesson) -> bool {
        if self.current_core() != Some(lesson) {
            return false;
        }
        self.completed_core.insert(lesson)
    }

    pub fn core_completed(&self, lesson: CoreLesson) -> bool {
        self.completed_core.contains(&lesson)
    }

    pub fn contracts_unlocked(&self) -> bool {
        self.current_core().is_none()
    }

    pub fn unlock_capability(&mut self, capability: Capability) -> bool {
        self.unlocked_capabilities.insert(capability)
    }

    pub fn capability_unlocked(&self, capability: Capability) -> bool {
        self.unlocked_capabilities.contains(&capability)
    }

    /// Devuelve cada oferta una sola vez, aunque se repita la compra.
    pub fn take_next_offer(&mut self) -> Option<Capability> {
        let capability = self
            .unlocked_capabilities
            .difference(&self.offered_capabilities)
            .next()
            .copied()?;
        self.offered_capabilities.insert(capability);
        Some(capability)
    }

    pub fn complete_capability_lesson(&mut self, capability: Capability) -> bool {
        if !self.capability_unlocked(capability) {
            return false;
        }
        self.offered_capabilities.insert(capability);
        self.completed_capability_lessons.insert(capability)
    }

    pub fn capability_lesson_completed(&self, capability: Capability) -> bool {
        self.completed_capability_lessons.contains(&capability)
    }

    pub fn available_capabilities(&self) -> impl Iterator<Item = Capability> + '_ {
        self.unlocked_capabilities.iter().copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn core_lessons_unlock_contracts_in_order() {
        let mut progress = TutorialProgress::new();
        assert!(!progress.contracts_unlocked());
        assert!(!progress.complete_core(CoreLesson::Investigation));

        for lesson in CORE_LESSONS {
            assert_eq!(progress.current_core(), Some(lesson));
            assert!(progress.complete_core(lesson));
        }

        assert!(progress.contracts_unlocked());
        assert!(!progress.complete_core(CoreLesson::Terminal));
    }

    #[test]
    fn unlocked_capability_is_offered_once_without_gating_it() {
        let mut progress = TutorialProgress::new();
        assert!(progress.unlock_capability(Capability::ServiceFoothold));
        assert!(progress.capability_unlocked(Capability::ServiceFoothold));
        assert_eq!(
            progress.take_next_offer(),
            Some(Capability::ServiceFoothold)
        );
        assert_eq!(progress.take_next_offer(), None);
        assert!(!progress.unlock_capability(Capability::ServiceFoothold));
        assert!(progress.complete_capability_lesson(Capability::ServiceFoothold));
        assert!(progress.capability_lesson_completed(Capability::ServiceFoothold));
    }
}
