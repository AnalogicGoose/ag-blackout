use crate::career::CoreLesson;
use crate::career::LessonId;
use crate::filesystem::VirtualPath;

use super::output::LineResult;
use super::scenario;
use super::session::{Shell, ShellEvent};

pub struct LessonRun {
    pub id: LessonId,
    shell: Shell,
    completed: bool,
}

pub struct GameSession {
    campaign: Shell,
    lesson: Option<LessonRun>,
}

impl GameSession {
    pub fn new(campaign: Shell) -> Self {
        Self {
            campaign,
            lesson: None,
        }
    }

    pub fn campaign(&self) -> &Shell {
        &self.campaign
    }

    pub fn campaign_mut(&mut self) -> &mut Shell {
        &mut self.campaign
    }

    pub fn active_lesson(&self) -> Option<LessonId> {
        self.lesson.as_ref().map(|run| run.id)
    }

    pub fn lesson_completed(&self) -> bool {
        self.lesson.as_ref().is_some_and(|run| run.completed)
    }

    pub fn active_shell(&self) -> &Shell {
        self.lesson
            .as_ref()
            .map_or(&self.campaign, |run| &run.shell)
    }

    pub fn active_shell_mut(&mut self) -> &mut Shell {
        match &mut self.lesson {
            Some(run) => &mut run.shell,
            None => &mut self.campaign,
        }
    }

    pub fn start_lesson(&mut self, id: LessonId) -> Result<(), String> {
        if self.lesson.is_some() {
            return Err("finish or leave the current lesson first".into());
        }

        let available = match id {
            LessonId::Core(lesson) => self.campaign.career.tutorial.core_available(lesson),
            LessonId::Capability(capability) => self
                .campaign
                .career
                .tutorial
                .capability_unlocked(capability),
        };
        if !available {
            return Err("lesson is locked".into());
        }

        let shell = match id {
            LessonId::Core(CoreLesson::Terminal) => scenario::terminal_practice(),
            _ => return Err("lesson content is not available yet".into()),
        };

        self.lesson = Some(LessonRun {
            id,
            shell,
            completed: false,
        });
        Ok(())
    }

    pub fn leave_lesson(&mut self) -> bool {
        self.lesson.take().is_some()
    }

    pub fn execute_line(&mut self, line: &str) -> LineResult {
        let Some(run) = self.lesson.as_mut() else {
            return self.campaign.execute_line(line);
        };

        let result = run.shell.execute_line(line);
        let events = run.shell.take_events();

        if run.id == LessonId::Core(CoreLesson::Terminal) && !run.completed {
            let target =
                VirtualPath::resolve(&VirtualPath::root(), "/home/guest/intro.txt").unwrap();

            let goal_met = events.iter().any(|event| {
                matches!(
                    event,
                    ShellEvent::FileRead { hostname, path }
                        if hostname == "training-local" && path == &target
                )
            });

            if goal_met {
                run.completed = true;
                self.campaign
                    .career
                    .tutorial
                    .complete_core(CoreLesson::Terminal);
            }
        }

        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn practice_world_does_not_modify_the_campaign() {
        let mut game = GameSession::new(scenario::tutorial());
        let contracts = game.campaign().contracts.available().count();

        game.start_lesson(LessonId::Core(CoreLesson::Terminal))
            .unwrap();
        assert_eq!(game.active_shell().active_hostname(), "training-local");

        let result = game.execute_line("cat /home/guest/intro.txt");
        assert_eq!(result.exit_code, 0);
        assert!(result.stdout.contains("Welcome to AG Linux"));

        game.active_shell_mut().economy.deposit(500);
        assert!(game.leave_lesson());

        assert_eq!(game.campaign().contracts.available().count(), contracts);
        assert_eq!(game.campaign().economy.balance(), 0);
        assert!(game.campaign().network.get("training-local").is_none());
    }

    #[test]
    fn terminal_lesson_requires_a_successful_read_and_can_be_replayed() {
        let mut game = GameSession::new(scenario::tutorial());
        let lesson = LessonId::Core(CoreLesson::Terminal);

        game.start_lesson(lesson).unwrap();

        assert_ne!(game.execute_line("cat missing.txt").exit_code, 0);
        assert!(!game.lesson_completed());

        assert_eq!(game.execute_line("cat intro.txt").exit_code, 0);
        assert!(game.lesson_completed());
        assert!(
            game.campaign()
                .career
                .tutorial
                .core_completed(CoreLesson::Terminal)
        );

        game.leave_lesson();
        game.start_lesson(lesson).unwrap();
        assert!(!game.lesson_completed());
        assert_eq!(game.execute_line("cat /home/guest/intro.txt").exit_code, 0);
        assert!(game.lesson_completed());
    }
}
