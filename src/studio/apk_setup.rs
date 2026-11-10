use super::{BAD, GOOD, egui};
use crate::bundle::apk::{Dependency, InstallProgress, Setup};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc,
};

#[derive(Default)]
pub(super) struct Panel {
    state: State,
    accepted_sdk: bool,
}

impl Drop for Panel {
    fn drop(&mut self) {
        if let State::Working {
            cancel: Some(cancel),
            ..
        } = &self.state
        {
            cancel.store(true, Ordering::Relaxed);
        }
    }
}

#[derive(Default)]
enum State {
    #[default]
    Idle,
    Working {
        receiver: mpsc::Receiver<Event>,
        cancel: Option<Arc<AtomicBool>>,
        progress: Option<InstallProgress>,
    },
    Available(Setup),
    Failed(String),
}

enum Event {
    Progress(InstallProgress),
    Finished(anyhow::Result<Setup>),
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Outcome {
    Waiting,
    Ready,
    Continue,
}

impl Panel {
    pub(super) fn installing(&self) -> bool {
        matches!(
            self.state,
            State::Working {
                cancel: Some(_),
                ..
            }
        )
    }

    fn start(&mut self, ctx: &egui::Context, install: bool) {
        let (sender, receiver) = mpsc::channel();
        let ctx = ctx.clone();
        let accepted_sdk = self.accepted_sdk;
        let cancel = install.then(|| Arc::new(AtomicBool::new(false)));
        let worker_cancel = cancel.clone();
        let worker = std::thread::Builder::new()
            .name("apk-setup".into())
            .spawn(move || {
                let result = std::env::current_exe()
                    .map_err(anyhow::Error::from)
                    .and_then(|exe| {
                        if let Some(cancel) = worker_cancel {
                            Setup::install(&exe, accepted_sdk, &cancel, |progress| {
                                let _ = sender.send(Event::Progress(progress));
                                ctx.request_repaint();
                            })
                        } else {
                            Setup::detect(&exe)
                        }
                    });
                let _ = sender.send(Event::Finished(result));
                ctx.request_repaint();
            });
        self.state = match worker {
            Ok(_) => State::Working {
                receiver,
                cancel,
                progress: None,
            },
            Err(error) => State::Failed(format!("无法启动 APK 环境任务。{error}")),
        };
    }

    fn poll(&mut self) -> bool {
        let State::Working {
            receiver,
            cancel,
            progress,
        } = &mut self.state
        else {
            return false;
        };
        let finished = loop {
            match receiver.try_recv() {
                Ok(Event::Progress(event)) => *progress = Some(event),
                Ok(Event::Finished(result)) => break result,
                Err(mpsc::TryRecvError::Empty) => return false,
                Err(mpsc::TryRecvError::Disconnected) => {
                    break Err(anyhow::anyhow!("APK 环境任务已中断"));
                }
            }
        };
        let cancelled = cancel
            .as_ref()
            .is_some_and(|flag| flag.load(Ordering::Relaxed));
        let resume = cancel.is_some() && !cancelled && finished.is_ok();
        self.state = match finished {
            Ok(setup) => State::Available(setup),
            Err(_) if cancelled => {
                State::Failed("安装已取消。重新检测后可以继续安装剩余组件。".into())
            }
            Err(error) => State::Failed(format!("{error:#}")),
        };
        resume
    }

    pub(super) fn show(&mut self, ui: &mut egui::Ui, can_export: bool) -> Outcome {
        if matches!(self.state, State::Idle) {
            self.start(ui.ctx(), false);
        }
        let resume = self.poll();
        ui.separator();
        ui.strong("APK 导出环境");
        let mut install = false;
        let mut retry = false;
        let mut outcome = Outcome::Waiting;
        match &self.state {
            State::Idle => {}
            State::Working {
                cancel, progress, ..
            } => {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(match (cancel, progress) {
                        (None, _) => "正在检测本机 JDK 和 Android SDK…".into(),
                        (Some(_), None) => "正在准备安装…".into(),
                        (_, Some(InstallProgress::Download { dependency, .. })) => {
                            format!("正在下载 {}", dependency.label())
                        }
                        (_, Some(InstallProgress::Extract(dependency))) => {
                            format!("已校验，正在解包 {}", dependency.label())
                        }
                    });
                });
                if let Some(InstallProgress::Download {
                    dependency,
                    received,
                }) = progress
                {
                    let total = dependency.download_bytes();
                    ui.add(
                        egui::ProgressBar::new(*received as f32 / total as f32).text(format!(
                            "{:.0} / {:.0} MB",
                            mb(*received),
                            mb(total)
                        )),
                    );
                }
                if let Some(cancel) = cancel {
                    let cancelling = cancel.load(Ordering::Relaxed);
                    let text = if cancelling {
                        "正在取消…"
                    } else {
                        "取消安装"
                    };
                    if ui
                        .add_enabled(!cancelling, egui::Button::new(text))
                        .clicked()
                    {
                        cancel.store(true, Ordering::Relaxed);
                    }
                }
            }
            State::Available(setup) => {
                let missing = setup.missing().collect::<Vec<_>>();
                if missing.is_empty() {
                    ui.colored_label(GOOD, "JDK 与 Android SDK 已就绪");
                    outcome = if resume && can_export {
                        Outcome::Continue
                    } else {
                        Outcome::Ready
                    };
                } else {
                    for dependency in Dependency::ALL {
                        let (status, color) = if setup.location(dependency).is_some() {
                            ("已找到", GOOD)
                        } else {
                            ("待安装", BAD)
                        };
                        ui.horizontal_wrapped(|ui| {
                            ui.colored_label(color, status);
                            ui.label(dependency.label());
                        });
                    }
                    let bytes = missing.iter().map(|item| item.download_bytes()).sum();
                    ui.label(format!(
                        "预计下载 {:.0} MB，安装到当前用户的 OpenGalgame 数据目录。",
                        mb(bytes)
                    ));
                    let needs_sdk = missing
                        .iter()
                        .any(|dependency| *dependency != Dependency::Jdk);
                    if needs_sdk {
                        ui.hyperlink_to(
                            "阅读 Android SDK 许可条款",
                            "https://developer.android.com/studio/terms",
                        );
                        ui.checkbox(
                            &mut self.accepted_sdk,
                            "我已阅读并同意 Android SDK 许可条款",
                        );
                    }
                    install = ui
                        .add_enabled(
                            can_export && (!needs_sdk || self.accepted_sdk),
                            egui::Button::new("安装并继续导出"),
                        )
                        .clicked();
                    retry = ui.button("重新检测").clicked();
                }
            }
            State::Failed(error) => {
                ui.colored_label(BAD, error);
                retry = ui.button("重新检测").clicked();
            }
        }
        if install {
            self.start(ui.ctx(), true);
        } else if retry {
            self.start(ui.ctx(), false);
        }
        outcome
    }
}

fn mb(bytes: u64) -> f64 {
    bytes as f64 / 1_000_000.
}
