//! 主线程计划任务调度；批处理延时使用定时器推进，不阻塞消息循环。
use crate::{
    schedule::{self, Command, Event, ReminderOptions, Step, Task},
    APP_STATE,
};
use std::{
    collections::VecDeque,
    time::{Duration, Instant},
};
use windows_sys::Win32::{Foundation::HWND, UI::WindowsAndMessaging::DestroyWindow};
pub const TIMER: usize = 31;
#[link(name = "wininet")]
extern "system" {
    fn InternetGetConnectedState(flags: *mut u32, reserved: u32) -> i32;
}
fn connected() -> bool {
    let mut flags = 0;
    unsafe { InternetGetConnectedState(&mut flags, 0) != 0 }
}
struct Job {
    id: String,
    name: String,
    reminder: Option<(String, ReminderOptions)>,
    steps: VecDeque<Step>,
    ready: Instant,
}
pub struct Runtime {
    pub session: String,
    previous: i64,
    network: bool,
    started: bool,
    exiting: bool,
    busy: bool,
    jobs: Vec<Job>,
    reported_error: bool,
}
impl Runtime {
    pub fn new() -> Self {
        Self {
            session: format!(
                "{}-{}",
                std::process::id(),
                crate::local_clock::now().timestamp_micros()
            ),
            previous: crate::local_clock::now().timestamp(),
            network: connected(),
            started: false,
            exiting: false,
            busy: false,
            jobs: Vec::new(),
            reported_error: false,
        }
    }
    /// 编辑、删除或恢复计划时，丢弃尚未执行的旧批处理步骤。
    pub fn cancel(&mut self, id: &str) {
        self.jobs.retain(|job| job.id != id);
    }
    pub fn reset_pending(&mut self) {
        self.jobs.clear();
        self.previous = crate::local_clock::now().timestamp();
        self.network = connected();
        self.reported_error = false;
    }
    fn enqueue(&mut self, task: Task) {
        if self.jobs.iter().any(|job| job.id == task.id) {
            return;
        }
        let has_reminder = task.has_reminder();
        self.jobs.push(Job {
            id: task.id,
            name: task.name,
            reminder: has_reminder.then_some((task.reminder, task.reminder_options)),
            steps: task.steps.into(),
            ready: Instant::now(),
        });
    }
}
enum Effect {
    Launch(Command),
    Reminder(String, String, ReminderOptions),
    Close,
}
fn collect(event: Event) -> Result<(), String> {
    APP_STATE.with(|cell| {
        let mut guard = cell
            .try_borrow_mut()
            .map_err(|_| "计划任务状态正忙。".to_string())?;
        let app = guard.as_mut().ok_or("应用尚未初始化。")?;
        let now = crate::local_clock::now();
        let mut plan = app.plan.clone();
        let mut due = Vec::new();
        let mut changed = false;
        for task in &mut plan.tasks {
            if event == Event::Startup
                && task.trigger == schedule::Trigger::ThisExit
                && task.session != app.scheduler.session
                && task.enabled
            {
                task.enabled = false;
                changed = true;
            }
            if task.due(event, now, app.scheduler.previous, &app.scheduler.session) {
                due.push(task.clone());
                changed = true;
            }
        }
        if changed {
            schedule::save(&app.base_dir, &plan)
                .map_err(|e| format!("保存计划任务失败，暂未执行：{e}"))?;
            app.plan = plan;
        }
        // 正常退出只等待退出计划，取消其他任务尚未执行的延时步骤。
        if event == Event::Exit {
            app.scheduler.jobs.clear();
        }
        for task in due {
            app.scheduler.enqueue(task);
        }
        if event == Event::Tick {
            app.scheduler.previous = now.timestamp();
        }
        Ok(())
    })
}
fn report_error(hwnd: HWND, error: String) {
    let report = APP_STATE
        .with(|cell| {
            let mut guard = cell.try_borrow_mut().ok()?;
            let scheduler = &mut guard.as_mut()?.scheduler;
            let report = !scheduler.reported_error;
            scheduler.reported_error = true;
            Some(report)
        })
        .unwrap_or(false);
    if report {
        crate::advanced_runtime::message(hwnd, &error);
    }
}
pub fn startup(hwnd: HWND) {
    let started = APP_STATE
        .with(|cell| {
            cell.try_borrow()
                .ok()
                .and_then(|guard| guard.as_ref().map(|app| app.scheduler.started))
        })
        .unwrap_or(true);
    if started {
        return;
    }
    if let Err(error) = collect(Event::Startup) {
        report_error(hwnd, error);
        return;
    }
    APP_STATE.with(|cell| {
        if let Ok(mut guard) = cell.try_borrow_mut() {
            if let Some(app) = guard.as_mut() {
                app.scheduler.started = true;
            }
        }
    });
    tick(hwnd);
}
pub fn begin_exit(hwnd: HWND) {
    let closing = APP_STATE
        .with(|cell| {
            cell.try_borrow()
                .ok()
                .and_then(|guard| guard.as_ref().map(|app| app.scheduler.exiting))
        })
        .unwrap_or(true);
    if closing {
        return;
    }
    if let Err(error) = collect(Event::Exit) {
        report_error(hwnd, error);
        return;
    }
    crate::schedule_reminder::close_all();
    APP_STATE.with(|cell| {
        if let Ok(mut guard) = cell.try_borrow_mut() {
            if let Some(app) = guard.as_mut() {
                app.scheduler.exiting = true;
            }
        }
    });
    tick(hwnd);
}
pub fn test(hwnd: HWND, mut task: Task) {
    task.id = format!("preview-{}", crate::local_clock::now().timestamp_micros());
    APP_STATE.with(|cell| {
        if let Ok(mut guard) = cell.try_borrow_mut() {
            if let Some(app) = guard.as_mut() {
                app.scheduler.enqueue(task);
            }
        }
    });
    tick(hwnd);
}
pub fn tick(hwnd: HWND) {
    let ready = APP_STATE.with(|cell| {
        cell.try_borrow().ok().and_then(|guard| {
            guard.as_ref().map(|app| {
                (
                    !app.scheduler.busy,
                    app.scheduler.exiting,
                    app.scheduler.started,
                )
            })
        })
    });
    let Some((true, exiting, started)) = ready else {
        return;
    };
    if !started {
        startup(hwnd);
        return;
    }
    if !exiting {
        if let Err(error) = collect(Event::Tick) {
            report_error(hwnd, error);
            return;
        }
        let network = connected();
        let changed = APP_STATE
            .with(|cell| {
                cell.try_borrow()
                    .ok()
                    .and_then(|guard| guard.as_ref().map(|app| app.scheduler.network != network))
            })
            .unwrap_or(false);
        if changed {
            if let Err(error) = collect(if network {
                Event::NetworkUp
            } else {
                Event::NetworkDown
            }) {
                report_error(hwnd, error);
                return;
            }
            APP_STATE.with(|cell| {
                if let Ok(mut guard) = cell.try_borrow_mut() {
                    if let Some(app) = guard.as_mut() {
                        app.scheduler.network = network;
                    }
                }
            });
        }
    }
    let effects = APP_STATE.with(|cell| {
        let mut guard = cell.try_borrow_mut().ok()?;
        let app = guard.as_mut()?;
        let runtime = &mut app.scheduler;
        runtime.busy = true;
        let now = Instant::now();
        let mut effects = Vec::new();
        for job in &mut runtime.jobs {
            if job.ready > now {
                continue;
            }
            if let Some((message, options)) = job.reminder.take() {
                effects.push(Effect::Reminder(job.name.clone(), message, options));
            }
            // 一轮只推进一步，消息循环持续处理绘制、编辑和其他计划任务。
            match job.steps.pop_front() {
                Some(Step::Launch { command }) => effects.push(Effect::Launch(command)),
                Some(Step::Delay { milliseconds }) => {
                    job.ready = now + Duration::from_millis(u64::from(milliseconds))
                }
                None => {}
            }
        }
        runtime
            .jobs
            .retain(|job| !job.steps.is_empty() || job.reminder.is_some() || job.ready > now);
        if runtime.exiting && runtime.jobs.is_empty() {
            effects.push(Effect::Close);
        }
        Some((effects, app.base_dir.clone(), app.user_cfg.advanced.clone()))
    });
    if let Some((effects, base, settings)) = effects {
        for effect in effects {
            match effect {
                Effect::Launch(command) => {
                    if !crate::launch_typed_target(
                        crate::targets::TargetKind::infer(&base, &command.link),
                        &command.link,
                        &command.args,
                        &command.work_dir,
                        hwnd,
                        &base,
                        &settings,
                    ) {
                        crate::advanced_runtime::message(
                            hwnd,
                            &format!("计划任务执行失败：{}", command.link),
                        );
                    }
                }
                Effect::Reminder(name, message, options) => {
                    if let Err(error) =
                        crate::schedule_reminder::show(hwnd, &base, &name, &message, options)
                    {
                        crate::advanced_runtime::message(hwnd, &error);
                    }
                }
                Effect::Close => unsafe {
                    if !crate::schedule_reminder::has_active() {
                        DestroyWindow(hwnd);
                    }
                },
            }
        }
    }
    APP_STATE.with(|cell| {
        if let Ok(mut guard) = cell.try_borrow_mut() {
            if let Some(app) = guard.as_mut() {
                app.scheduler.busy = false;
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn active_occurrences_are_coalesced_and_cancelling_removes_remaining_steps() {
        let mut runtime = Runtime::new();
        let mut task = Task::new(crate::local_clock::now(), &runtime.session);
        task.name = "运行队列".into();
        task.reminder = "回归".into();
        runtime.enqueue(task.clone());
        runtime.enqueue(task.clone());
        assert_eq!(runtime.jobs.len(), 1);
        runtime.cancel(&task.id);
        assert!(runtime.jobs.is_empty());
        runtime.enqueue(task);
        runtime.reset_pending();
        assert!(runtime.jobs.is_empty());
    }
}
