use anyhow::{Context, Result};
use rodio::{OutputStream, OutputStreamBuilder, Sink, Source};
use std::{path::Path, time::Instant};

#[derive(Clone, Copy, Debug)]
pub struct Envelope {
    pub from: f32,
    pub to: f32,
    pub seconds: f32,
}
impl Envelope {
    pub fn at(self, elapsed: f32) -> f32 {
        if self.seconds <= 0. {
            self.to
        } else {
            self.from + (self.to - self.from) * (elapsed / self.seconds).clamp(0., 1.)
        }
    }
}
struct TrackPlayer {
    sink: Sink,
    key: String,
    envelope: Envelope,
    started: Instant,
}
struct EffectPlayer {
    sink: Sink,
    volume: f32,
}
pub struct SoundEngine {
    stream: Option<OutputStream>,
    music: Option<TrackPlayer>,
    ambience: Option<TrackPlayer>,
    retiring: Vec<(TrackPlayer, bool)>,
    voice: Option<Sink>,
    effects: Vec<EffectPlayer>,
    levels: [f32; 4],
    pub duck_voice: bool,
    pub error: Option<String>,
}
impl Default for SoundEngine {
    fn default() -> Self {
        Self {
            stream: None,
            music: None,
            ambience: None,
            retiring: vec![],
            voice: None,
            effects: vec![],
            levels: [1.; 4],
            duck_voice: true,
            error: None,
        }
    }
}
impl SoundEngine {
    fn init(&mut self) -> Result<()> {
        if self.stream.is_none() {
            self.stream = Some(
                OutputStreamBuilder::open_default_stream().context("找不到可用的音频输出设备")?,
            );
        }
        Ok(())
    }
    pub fn track(
        &mut self,
        ambient: bool,
        path: Option<&Path>,
        volume: f32,
        looped: bool,
        fade: f32,
    ) -> Result<()> {
        let key = path.map(|p| format!("{}|{looped}", p.display()));
        let current = if ambient {
            &mut self.ambience
        } else {
            &mut self.music
        };
        if current.as_ref().map(|t| t.key.as_str()) == key.as_deref() {
            if let Some(track) = current
                && (track.envelope.to - volume).abs() > 0.001
            {
                track.envelope = Envelope {
                    from: track.envelope.at(track.started.elapsed().as_secs_f32()),
                    to: volume.clamp(0., 1.),
                    seconds: fade,
                };
                track.started = Instant::now();
            }
            return Ok(());
        }
        let replacement = if let Some(path) = path {
            self.init()?;
            let source = rodio::Decoder::try_from(std::fs::File::open(path)?)?;
            let sink = Sink::connect_new(self.stream.as_ref().unwrap().mixer());
            sink.set_volume(if fade > 0. { 0. } else { volume.clamp(0., 1.) });
            if looped {
                sink.append(source.repeat_infinite());
            } else {
                sink.append(source);
            }
            Some(TrackPlayer {
                sink,
                key: key.unwrap(),
                envelope: Envelope {
                    from: if fade > 0. { 0. } else { volume },
                    to: volume.clamp(0., 1.),
                    seconds: fade.max(0.),
                },
                started: Instant::now(),
            })
        } else {
            None
        };
        let current = if ambient {
            &mut self.ambience
        } else {
            &mut self.music
        };
        if let Some(mut old) = current.take() {
            if fade > 0. {
                old.envelope = Envelope {
                    from: old.envelope.at(old.started.elapsed().as_secs_f32()),
                    to: 0.,
                    seconds: fade,
                };
                old.started = Instant::now();
                self.retiring.push((old, ambient));
                if self.retiring.len() > 8 {
                    let (old, _) = self.retiring.remove(0);
                    old.sink.stop();
                }
            } else {
                old.sink.stop();
            }
        }
        if ambient {
            self.ambience = replacement;
        } else {
            self.music = replacement;
        }
        self.tick();
        Ok(())
    }
    pub fn music(&mut self, path: Option<&Path>, volume: f32) -> Result<()> {
        self.track(false, path, volume, true, 0.)
    }
    pub fn effect(&mut self, path: &Path, volume: f32) -> Result<()> {
        self.init()?;
        let source = rodio::Decoder::try_from(std::fs::File::open(path)?)?;
        let sink = Sink::connect_new(self.stream.as_ref().unwrap().mixer());
        sink.set_volume(volume.clamp(0., 1.) * self.levels[3]);
        sink.append(source);
        self.effects.retain(|p| !p.sink.empty());
        if self.effects.len() >= 8 {
            self.effects.remove(0).sink.stop();
        }
        self.effects.push(EffectPlayer {
            sink,
            volume: volume.clamp(0., 1.),
        });
        Ok(())
    }
    pub fn voice(&mut self, path: &Path) -> Result<()> {
        self.stop_voice();
        self.init()?;
        let source = rodio::Decoder::try_from(std::fs::File::open(path)?)?;
        let sink = Sink::connect_new(self.stream.as_ref().unwrap().mixer());
        sink.set_volume(self.levels[2]);
        sink.append(source);
        self.voice = Some(sink);
        self.tick();
        Ok(())
    }
    pub fn voice_playing(&self) -> bool {
        self.voice.as_ref().is_some_and(|s| !s.empty())
    }
    pub fn fading(&self) -> bool {
        !self.retiring.is_empty()
            || [&self.music, &self.ambience]
                .into_iter()
                .flatten()
                .any(|t| t.started.elapsed().as_secs_f32() < t.envelope.seconds)
    }
    pub fn set_levels(&mut self, music: f32, ambience: f32, voice: f32, effects: f32) {
        self.levels = [music, ambience, voice, effects].map(|v| v.clamp(0., 1.));
        self.tick();
    }
    pub fn volumes(&mut self, music: f32, effects: f32) {
        self.levels[0] = music.clamp(0., 1.);
        self.levels[3] = effects.clamp(0., 1.);
        self.tick();
    }
    pub fn tick(&mut self) {
        let duck = if self.duck_voice && self.voice_playing() {
            0.45
        } else {
            1.
        };
        for (track, level) in [
            (&self.music, self.levels[0] * duck),
            (&self.ambience, self.levels[1]),
        ] {
            if let Some(t) = track {
                t.sink
                    .set_volume(t.envelope.at(t.started.elapsed().as_secs_f32()) * level);
            }
        }
        self.retiring.retain(|(t, _)| {
            if t.started.elapsed().as_secs_f32() >= t.envelope.seconds {
                t.sink.stop();
                false
            } else {
                true
            }
        });
        for (t, ambient) in &self.retiring {
            t.sink.set_volume(
                t.envelope.at(t.started.elapsed().as_secs_f32())
                    * if *ambient {
                        self.levels[1]
                    } else {
                        self.levels[0] * duck
                    },
            );
        }
        if let Some(s) = &self.voice {
            s.set_volume(self.levels[2]);
        }
        self.effects.retain(|p| !p.sink.empty());
        for p in &self.effects {
            p.sink.set_volume(p.volume * self.levels[3]);
        }
    }
    pub fn stop_voice(&mut self) {
        if let Some(s) = self.voice.take() {
            s.stop();
        }
    }
    pub fn stop_effects(&mut self) {
        for p in self.effects.drain(..) {
            p.sink.stop();
        }
    }
    pub fn stop(&mut self) {
        self.stop_voice();
        self.stop_effects();
        for s in [&mut self.music, &mut self.ambience] {
            if let Some(p) = s.take() {
                p.sink.stop();
            }
        }
        for (p, _) in self.retiring.drain(..) {
            p.sink.stop();
        }
    }
}
