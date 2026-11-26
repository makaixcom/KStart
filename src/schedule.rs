//! 计划任务的数据、校验与触发判定；所有时间均按 Windows 本地时区解释。
use chrono::{DateTime, Datelike, FixedOffset, NaiveDate, NaiveTime};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Trigger {
    Daily,
    Weekly,
    Monthly,
    Yearly,
    Once,
    Interval,
    Startup,
    NextStartup,
    Exit,
    ThisExit,
    NetworkUp,
    NetworkDown,
    Countdown,
}
impl Trigger {
    pub const ALL: [Self; 13] = [
        Self::Daily,
        Self::Weekly,
        Self::Monthly,
        Self::Yearly,
        Self::Once,
        Self::Interval,
        Self::Startup,
        Self::NextStartup,
        Self::Exit,
        Self::ThisExit,
        Self::NetworkUp,
        Self::NetworkDown,
        Self::Countdown,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::Daily => "每天",
            Self::Weekly => "每周",
            Self::Monthly => "每月",
            Self::Yearly => "每年",
            Self::Once => "准确时间",
            Self::Interval => "每隔一定时间",
            Self::Startup => "每次 KStart 启动时",
            Self::NextStartup => "下次 KStart 启动时",
            Self::Exit => "每次 KStart 结束时",
            Self::ThisExit => "本次 KStart 结束时",
            Self::NetworkUp => "网络连通",
            Self::NetworkDown => "网络断开",
            Self::Countdown => "倒计时",
        }
    }
    pub fn duration(self) -> bool {
        matches!(self, Self::Interval | Self::Countdown)
    }
    pub fn calendar(self) -> bool {
        matches!(
            self,
            Self::Daily | Self::Weekly | Self::Monthly | Self::Yearly | Self::Once
        )
    }
}
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Command {
    pub link: String,
    pub args: String,
    pub work_dir: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Step {
    Launch { command: Command },
    Delay { milliseconds: u32 },
}
/// 声音文件只保存来源路径；旧计划缺少这些选项时使用默认值。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ReminderOptions {
    pub allow_auto_close: bool,
    pub play_sound: bool,
    pub sound_file: String,
}
impl ReminderOptions {
    pub fn validate(&self, text: &str) -> Result<(), String> {
        if text.chars().count() > 500 {
            return Err("提醒内容不能超过 500 字。".into());
        }
        if self.play_sound && self.sound_file.trim().is_empty() {
            return Err("请选择提醒声音文件。".into());
        }
        Ok(())
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: String,
    pub name: String,
    pub enabled: bool,
    pub trigger: Trigger,
    pub date: String,
    pub time: String,
    pub seconds: u32,
    pub steps: Vec<Step>,
    pub reminder: String,
    #[serde(default)]
    pub reminder_options: ReminderOptions,
    #[serde(default)]
    pub session: String,
    #[serde(default)]
    pub next_run_ms: Option<i64>,
    #[serde(default)]
    pub last_run: Option<i64>,
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Event {
    Tick,
    Startup,
    Exit,
    NetworkUp,
    NetworkDown,
}
impl Task {
    pub fn new(now: DateTime<FixedOffset>, session: &str) -> Self {
        let initial = now + chrono::Duration::minutes(1);
        Self {
            id: format!("task_{}", now.timestamp_micros()),
            name: String::new(),
            enabled: true,
            trigger: Trigger::Once,
            date: initial.format("%Y-%m-%d").to_string(),
            time: initial.format("%H:%M:%S").to_string(),
            seconds: 60,
            steps: Vec::new(),
            reminder: String::new(),
            reminder_options: ReminderOptions::default(),
            session: session.into(),
            next_run_ms: None,
            last_run: None,
        }
    }
    pub fn has_reminder(&self) -> bool {
        !self.reminder.trim().is_empty() || self.reminder_options.play_sound
    }
    pub fn validate(&self) -> Result<(), String> {
        self.reminder_options.validate(&self.reminder)?;
        if self.name.trim().is_empty() {
            return Err("请输入任务名称。".into());
        }
        if self.steps.is_empty() && !self.has_reminder() {
            return Err("请填写链接、添加批处理或设置提醒。".into());
        }
        if self.steps.len() > 1000 {
            return Err("批处理最多包含 1000 个步骤。".into());
        }
        if self
            .steps
            .iter()
            .any(|s| matches!(s,Step::Launch{command} if command.link.trim().is_empty()))
        {
            return Err("批处理包含空链接。".into());
        }
        if self.trigger.duration() && self.seconds == 0 {
            return Err("间隔或倒计时必须大于零。".into());
        }
        if self.trigger.calendar()
            && (NaiveDate::parse_from_str(&self.date, "%Y-%m-%d").is_err()
                || NaiveTime::parse_from_str(&self.time, "%H:%M:%S").is_err())
        {
            return Err("日期或时间无效。".into());
        }
        Ok(())
    }
    pub fn arm(&mut self, now: DateTime<FixedOffset>, session: &str) {
        self.session = session.into();
        self.last_run = None;
        self.next_run_ms = self
            .trigger
            .duration()
            .then(|| now.timestamp_millis() + i64::from(self.seconds) * 1000);
    }
    pub fn summary(&self) -> String {
        if !self.enabled {
            return "已停用".into();
        }
        if self.trigger.duration() {
            return format!(
                "{}时{}分{}秒",
                self.seconds / 3600,
                self.seconds / 60 % 60,
                self.seconds % 60
            );
        }
        let day = NaiveDate::parse_from_str(&self.date, "%Y-%m-%d").ok();
        match self.trigger {
            Trigger::Daily => self.time.clone(),
            Trigger::Weekly => format!(
                "{} {}",
                day.map(|d| ["周一", "周二", "周三", "周四", "周五", "周六", "周日"]
                    [d.weekday().num_days_from_monday() as usize])
                    .unwrap_or(""),
                self.time
            ),
            Trigger::Monthly => format!("每月{}日 {}", day.map_or(1, |d| d.day()), self.time),
            Trigger::Yearly => format!("{} {}", self.date.get(5..).unwrap_or(""), self.time),
            Trigger::Once => format!("{} {}", self.date, self.time),
            _ => "事件触发".into(),
        }
    }
    /// 首次启动不补跑周期任务；休眠跨过执行时间时只补跑一次。
    pub fn due(
        &mut self,
        event: Event,
        now: DateTime<FixedOffset>,
        previous: i64,
        session: &str,
    ) -> bool {
        if !self.enabled || self.validate().is_err() {
            return false;
        }
        let stamp = now.timestamp();
        let fire = match self.trigger {
            Trigger::Startup => event == Event::Startup,
            Trigger::NextStartup => event == Event::Startup && self.session != session,
            Trigger::Exit => event == Event::Exit,
            Trigger::ThisExit => event == Event::Exit && self.session == session,
            Trigger::NetworkUp => event == Event::NetworkUp,
            Trigger::NetworkDown => event == Event::NetworkDown,
            Trigger::Interval | Trigger::Countdown => {
                event == Event::Tick
                    && self
                        .next_run_ms
                        .is_some_and(|t| now.timestamp_millis() >= t)
            }
            trigger if trigger.calendar() && event == Event::Tick => {
                let Ok(date) = NaiveDate::parse_from_str(&self.date, "%Y-%m-%d") else {
                    return false;
                };
                let Ok(time) = NaiveTime::parse_from_str(&self.time, "%H:%M:%S") else {
                    return false;
                };
                let today = now.date_naive();
                let matches = match trigger {
                    Trigger::Weekly => today.weekday() == date.weekday(),
                    Trigger::Monthly => today.day() == date.day(),
                    Trigger::Yearly => today.month() == date.month() && today.day() == date.day(),
                    _ => true,
                };
                let due_date = if trigger == Trigger::Once {
                    date
                } else {
                    today
                };
                let candidate = due_date.and_time(time).and_utc().timestamp()
                    - i64::from(now.offset().local_minus_utc());
                matches
                    && candidate <= stamp
                    && (trigger == Trigger::Once || candidate > previous)
                    && self.last_run.is_none_or(|last| last < candidate)
            }
            _ => false,
        };
        if fire {
            self.last_run = Some(stamp);
            if self.trigger == Trigger::Interval {
                let period = i64::from(self.seconds.max(1)) * 1000;
                let stamp = now.timestamp_millis();
                let next = self.next_run_ms.unwrap_or(stamp);
                self.next_run_ms = Some(next + ((stamp - next) / period + 1) * period);
            }
            if matches!(
                self.trigger,
                Trigger::Once | Trigger::Countdown | Trigger::NextStartup | Trigger::ThisExit
            ) {
                self.enabled = false;
            }
        }
        fire
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn at(s: &str) -> DateTime<FixedOffset> {
        DateTime::parse_from_rfc3339(s).unwrap()
    }
    fn task(trigger: Trigger) -> Task {
        let mut t = Task::new(at("2026-11-26T10:00:00+08:00"), "first");
        t.time = "10:00:00".into();
        t.name = "提醒".into();
        t.reminder = "测试".into();
        t.trigger = trigger;
        t
    }
    #[test]
    fn recurring_runs_once_after_sleep_and_monthly_skips_missing_day() {
        let mut t = task(Trigger::Daily);
        let now = at("2026-11-26T11:00:00+08:00");
        let before = at("2026-11-26T09:00:00+08:00").timestamp();
        assert!(t.due(Event::Tick, now, before, "first"));
        assert!(!t.due(Event::Tick, now, before, "first"));
        let mut t = task(Trigger::Monthly);
        t.date = "2026-01-31".into();
        assert!(!t.due(Event::Tick, at("2026-02-28T11:00:00+08:00"), 0, "first"));
    }
    #[test]
    fn interval_catches_up_without_burst_and_countdown_only_once() {
        let now = at("2026-11-26T10:00:00+08:00");
        let mut t = task(Trigger::Interval);
        t.arm(now, "first");
        let later = now + chrono::Duration::seconds(305);
        assert!(t.due(Event::Tick, later, 0, "first"));
        assert_eq!(t.next_run_ms, Some(now.timestamp_millis() + 360_000));
        assert!(!t.due(Event::Tick, later, 0, "first"));
        t.trigger = Trigger::Countdown;
        t.arm(now, "first");
        assert!(t.due(Event::Tick, later, 0, "first"));
        assert!(!t.enabled);
        let fractional = at("2026-11-26T10:00:00.950+08:00");
        t.enabled = true;
        t.seconds = 1;
        t.arm(fractional, "first");
        assert!(!t.due(
            Event::Tick,
            fractional + chrono::Duration::milliseconds(999),
            0,
            "first"
        ));
        assert!(t.due(
            Event::Tick,
            fractional + chrono::Duration::milliseconds(1000),
            0,
            "first"
        ));
    }
    #[test]
    fn startup_exit_and_network_events_respect_sessions() {
        let now = at("2026-11-26T10:00:00+08:00");
        let mut t = task(Trigger::NextStartup);
        assert!(!t.due(Event::Startup, now, 0, "first"));
        assert!(t.due(Event::Startup, now, 0, "second"));
        assert!(!t.enabled);
        let mut t = task(Trigger::ThisExit);
        assert!(!t.due(Event::Exit, now, 0, "second"));
        assert!(t.due(Event::Exit, now, 0, "first"));
        let mut t = task(Trigger::NetworkDown);
        assert!(!t.due(Event::NetworkUp, now, 0, "first"));
        assert!(t.due(Event::NetworkDown, now, 0, "first"));
    }
    #[test]
    fn one_shot_and_serialization_keep_execution_state() {
        let mut t = task(Trigger::Once);
        let now = at("2026-11-26T10:00:01+08:00");
        assert!(t.due(Event::Tick, now, 0, "first"));
        let mut loaded: Task = serde_json::from_str(&serde_json::to_string(&t).unwrap()).unwrap();
        assert!(!loaded.due(Event::Tick, now, 0, "first"));
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Plan {
    #[serde(default)]
    pub tasks: Vec<Task>,
}
pub fn load(base: &std::path::Path) -> std::io::Result<Plan> {
    match std::fs::read(base.join("user/plan.json")) {
        Ok(bytes) => serde_json::from_slice(&bytes).map_err(std::io::Error::other),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(Plan::default()),
        Err(err) => Err(err),
    }
}
pub fn save(base: &std::path::Path, plan: &Plan) -> std::io::Result<()> {
    let directory = base.join("user");
    std::fs::create_dir_all(&directory)?;
    let bytes = serde_json::to_vec_pretty(plan).map_err(std::io::Error::other)?;
    let temporary = directory.join("plan.json.tmp");
    let result = std::fs::write(&temporary, bytes)
        .and_then(|_| std::fs::rename(&temporary, directory.join("plan.json")));
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    result
}

#[cfg(test)]
mod persistence_tests {
    use super::*;
    #[test]
    fn plan_round_trip_is_independent_and_corrupt_file_is_not_overwritten() {
        let base = std::env::temp_dir().join(format!(
            "kstart-plan-store-{}-{}",
            std::process::id(),
            crate::local_clock::now().timestamp_micros()
        ));
        assert!(load(&base).unwrap().tasks.is_empty());
        assert!(!base.exists());
        std::fs::create_dir_all(base.join("user")).unwrap();
        std::fs::write(base.join("user/data.json"), b"shortcut-data").unwrap();
        let now = crate::local_clock::now();
        let mut task = Task::new(now, "test");
        task.name = "独立计划".into();
        task.reminder = "测试".into();
        task.trigger = Trigger::Countdown;
        task.seconds = 1;
        task.arm(now, "test");
        save(
            &base,
            &Plan {
                tasks: vec![task.clone()],
            },
        )
        .unwrap();
        let loaded = load(&base).unwrap();
        assert_eq!(loaded.tasks[0].id, task.id);
        assert_eq!(loaded.tasks[0].next_run_ms, task.next_run_ms);
        assert_eq!(
            std::fs::read(base.join("user/data.json")).unwrap(),
            b"shortcut-data"
        );
        assert!(!base.join("user/plan.json.tmp").exists());
        std::fs::write(base.join("user/plan.json"), b"broken-json").unwrap();
        assert!(load(&base).is_err());
        assert_eq!(
            std::fs::read(base.join("user/plan.json")).unwrap(),
            b"broken-json"
        );
        std::fs::remove_dir_all(base).unwrap();
    }
    #[test]
    fn weekly_yearly_and_disabled_tasks_use_the_selected_calendar_date() {
        let now = DateTime::parse_from_rfc3339("2028-02-29T10:00:00+08:00").unwrap();
        let mut task = Task::new(now, "test");
        task.name = "周期回归".into();
        task.reminder = "测试".into();
        task.date = "2024-02-29".into();
        task.time = "09:00:00".into();
        task.trigger = Trigger::Yearly;
        assert!(task.due(Event::Tick, now, now.timestamp() - 7200, "test"));
        assert!(!task.due(Event::Tick, now, now.timestamp() - 7200, "test"));
        task.trigger = Trigger::Weekly;
        task.last_run = None;
        task.date = "2028-02-22".into();
        assert!(task.due(Event::Tick, now, now.timestamp() - 7200, "test"));
        task.enabled = false;
        task.last_run = None;
        assert!(!task.due(Event::Tick, now, 0, "test"));
        task.seconds = 0;
        task.trigger = Trigger::Interval;
        assert!(task.validate().is_err());
    }
}

#[cfg(test)]
mod reminder_tests {
    use super::*;
    #[test]
    fn reminder_options_round_trip_and_old_text_records_keep_defaults() {
        let mut task = Task::new(crate::local_clock::now(), "test");
        task.name = "提醒".into();
        task.reminder = "旧提醒".into();
        let mut old = serde_json::to_value(&task).unwrap();
        old.as_object_mut().unwrap().remove("reminder_options");
        let decoded: Task = serde_json::from_value(old).unwrap();
        assert_eq!(decoded.reminder_options, ReminderOptions::default());
        task.reminder_options = ReminderOptions {
            allow_auto_close: true,
            play_sound: true,
            sound_file: "sounds/reminder.wav".into(),
        };
        let decoded: Task = serde_json::from_slice(&serde_json::to_vec(&task).unwrap()).unwrap();
        assert_eq!(decoded.reminder_options, task.reminder_options);
        assert!(decoded.validate().is_ok());
    }
    #[test]
    fn reminder_limit_and_sound_selection_are_validated() {
        let options = ReminderOptions::default();
        assert!(options.validate(&"字".repeat(500)).is_ok());
        assert!(options.validate(&"字".repeat(501)).is_err());
        let options = ReminderOptions {
            play_sound: true,
            ..Default::default()
        };
        assert!(options.validate("提醒").is_err());
        let mut task = Task::new(crate::local_clock::now(), "test");
        task.name = "声音提醒".into();
        task.reminder_options = ReminderOptions {
            play_sound: true,
            sound_file: "alarm.wav".into(),
            ..Default::default()
        };
        assert!(task.has_reminder());
        assert!(task.validate().is_ok());
    }
}
