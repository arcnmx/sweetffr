use {
	discord_presence::models::Activity,
	log::{debug, error, info, warn},
	reqwest::Url,
	std::{
		collections::HashMap,
		hash::{DefaultHasher, Hash, Hasher},
		time::{Duration, SystemTime, UNIX_EPOCH},
	},
	sweetffr::ffr::{
		stream::{StreamEvent, StreamJudge, StreamMessage, StreamSong},
		Player,
	},
	tokio::time::Instant,
};

pub type SongKey = (u64, u32);
pub fn song_key(song: &StreamSong) -> SongKey {
	let mut hasher = DefaultHasher::new();
	let engine_id = song.engine.as_ref().map(|engine| &engine.id);
	engine_id.hash(&mut hasher);
	let engine_id_hash = hasher.finish();
	(engine_id_hash, song.song.level)
}

#[derive(Debug, Default)]
pub struct Playing {
	pub now_playing: HashMap<SongKey, NowPlaying>,
	latest_playing: Option<SongKey>,
}

impl Playing {
	// TODO: Result<()>?
	pub fn process_event(&mut self, event: &StreamMessage) {
		match &event.event {
			StreamEvent::SongStart(song) => {
				let key = song_key(&song);
				let play = NowPlaying::start(song.clone());
				info!("Song started: {}", play.song.song.name);
				self.now_playing.insert(key, play);
				self.latest_playing = Some(key);
			},
			StreamEvent::SongRestart(song) =>
				if let Some(play) = self.now_playing_mut(&song) {
					play.restart(song.clone());
					info!("Song restart #{}", play.restarts);
				} else {
					error!("Not playing {song:?} to restart");
				},
			StreamEvent::SongEnd(song) => {
				if let Some(play) = self.now_playing_mut(&song) {
					play.end();
					info!("Song complete: {}", play.judge.judge.results());
				//latest_playing = None;
				} else {
					error!("Not playing {song:?} to end");
				}
			},
			StreamEvent::SongPause(song) =>
				if let Some(play) = self.now_playing_mut(&song) {
					info!("Game paused");
					play.pause();
				} else {
					error!("Not playing {song:?} to pause");
				},
			StreamEvent::SongResume(song) =>
				if let Some(play) = self.now_playing_mut(&song) {
					info!("Game resumed");
					play.resume();
				} else {
					error!("Not playing {song:?} to resume");
				},
			StreamEvent::NoteJudge(note) => {
				let play = self.latest_mut();
				if let Some(play) = play {
					play.note(note.clone());
					debug!(
						"Hit: ({} / {}) {}",
						play.elapsed().unwrap().as_secs(),
						play.song.song.time_seconds as u64,
						play.judge.judge.results()
					);
				} else {
					error!("Got NOTE_JUDGE but not playing anything");
				}
			},
		}
	}

	pub fn now_playing(&self, song: &'_ StreamSong) -> Option<&NowPlaying> {
		self.now_playing.get(&song_key(song))
	}

	pub fn now_playing_mut(&mut self, song: &'_ StreamSong) -> Option<&mut NowPlaying> {
		self.now_playing.get_mut(&song_key(song))
	}

	pub fn latest(&self) -> Option<&NowPlaying> {
		self.latest_playing.as_ref().and_then(|song| self.now_playing.get(song))
	}

	pub fn latest_mut(&mut self) -> Option<&mut NowPlaying> {
		self
			.latest_playing
			.as_ref()
			.and_then(|song| self.now_playing.get_mut(song))
	}

	pub fn player(&self) -> Option<&Player> {
		self.now_playing.iter().next().map(|(_, play)| &play.song.player)
	}

	pub fn discord_activity_idle(&self, play: Option<&NowPlaying>) -> Activity {
		let player = play.map(|play| &play.song.player).or_else(|| self.player());
		let ffr_icon = "https://www.flashflashrevolution.com/images/2008/ffr-site-icon.png";
		Activity::default().assets(move |assets| {
			let assets = match player {
				Some(player) => assets
					.small_image(player.avatar.clone())
					.small_text(format!("{} ({})", player.name, player.skill_level)),
				None => assets,
			};
			assets.large_image(String::from(ffr_icon))
		})
	}

	pub fn discord_activity(&self, play: Option<&NowPlaying>) -> Activity {
		let play = play.or_else(|| self.latest());
		let activity = self.discord_activity_idle(play);
		if let Some(play) = play {
			play.discord_activity()(activity)
		} else {
			activity
		}
	}
}

#[derive(Debug)]
pub struct NowPlaying {
	pub song: StreamSong,
	pub judge: StreamJudge,
	pub start: Option<Instant>,
	pub paused_for: Duration,
	pub paused: Option<Instant>,
	pub ended: Option<Instant>,
	// TODO: recent links expire eventually..
	pub recent_id: Option<u32>,
	pub restarts: u32,
	// TODO: encode pauses in here...
	#[cfg(replay_frames)]
	pub replay_frames: Vec<(Instant, u8)>,
}

impl NowPlaying {
	pub fn start(song: StreamSong) -> Self {
		Self {
			judge: Default::default(),
			start: Some(Instant::now()),
			paused_for: Default::default(),
			paused: None,
			ended: None,
			recent_id: None,
			restarts: 0,
			#[cfg(replay_frames)]
			replay_frames: Vec::with_capacity(song.song.note_count as usize),
			song,
		}
	}

	pub fn restart(&mut self, song: StreamSong) {
		let restarts = if self.song.song != song.song {
			warn!("restart: song changed???");
			0
		} else {
			self.restarts + 1
		};
		let recent_id = self.recent_id;
		*self = Self::start(song);
		self.restarts = restarts;
		self.recent_id = recent_id;
	}

	pub fn pause(&mut self) {
		if self.paused.is_some() {
			warn!("double-pause???");
			return
		}
		self.paused = Some(Instant::now());
	}

	pub fn resume(&mut self) {
		let paused = match self.paused.take() {
			None => {
				warn!("resume: we weren't paused?");
				return
			},
			Some(paused) => paused,
		};
		self.paused_for = self.paused_for + Instant::now().duration_since(paused);
	}

	pub fn end(&mut self) {
		if self.ended.is_some() {
			warn!("double-end???");
			return
		}

		self.ended = Some(Instant::now());
	}

	pub fn note(&mut self, note: StreamJudge) {
		if self.ended.is_some() {
			warn!("note: after end???");
			return
		} else if self.start.is_none() {
			warn!("note: before start???");
			return
		}

		#[cfg(replay_frames)]
		match note.last_hit {
			Some(-5) => {
				self.replay_frames.push((Instant::now(), b'W'));
				self.replay_frames.push((Instant::now(), b'X'));
				self.replay_frames.push((Instant::now(), b'Y'));
				self.replay_frames.push((Instant::now(), b'Z'));
			},
			Some(hit) if hit > 0 => {
				// TODO: W/X/Y/Z
				let direction = b'W';
				self.replay_frames.push((Instant::now(), direction));
				self.replay_frames.push((Instant::now(), b'Y'));
			},
			_ => (),
		}

		self.judge = note;
	}

	pub fn recent(&mut self, recent_id: u32) {
		if !self.ended.is_some() {
			warn!("recent: before end???");
			return
		}

		self.recent_id = Some(recent_id);
	}

	pub fn elapsed(&self) -> Option<Duration> {
		let end = self.ended.unwrap_or_else(|| Instant::now());
		Some(end.duration_since(self.start?) - self.paused_for)
	}

	pub fn discord_activity(&self) -> impl FnOnce(Activity) -> Activity {
		let level = &self.song.song;
		let judge = &self.judge.judge;

		let state = if self.paused.is_some() {
			Some("Paused".into())
		} else if self.start.is_none() {
			Some("Loading...".into())
		} else if judge.raw_score() <= 0 {
			None
		} else {
			let results = judge.results();
			let state = match judge.is_complete(level) {
				true => results,
				false => match judge.is_full_combo() {
					true => format!("{results}/{}", level.note_count),
					false => format!(
						"{results}({}) ({} / {})",
						self.judge.combo,
						judge.note_count(),
						level.note_count
					),
				},
			};
			let state = match judge.is_complete(level) {
				true if judge.is_aaaa() => format!("{state} (AAAA)"),
				true if judge.is_aaa() => format!("{state} (AAA)"),
				true if judge.is_full_combo() => format!("{state} (FC)"),
				_ => state,
			};
			let is_pb = !self.song.best_score.is_unplayed() && *judge > self.song.best_score.judge;
			let state = match is_pb {
				false => state,
				true => format!("{state} (PB)"),
			};
			Some(state)
		};

		let timestamp = if self.start.is_some() && self.ended.is_none() {
			let start = SystemTime::now() - self.elapsed().unwrap_or_default();
			let end = start + level.duration();
			let start = start.duration_since(UNIX_EPOCH).unwrap().as_secs();
			let end = end.duration_since(UNIX_EPOCH).unwrap().as_secs();
			Some((start, end))
		} else {
			None
		};

		let title = if state.is_some() || timestamp.is_some() || self.start.is_some() {
			let title = format!("{} ({})", level.name.trim(), level.difficulty);
			let title = match self.song.player.settings.rate() {
				Err(e) => {
					warn!("Failed to parse PlayerSettings.rate: {e:?}");
					title
				},
				Ok(None) => title,
				Ok(Some(rate)) => format!("{title} ({rate:.2}x)"),
			};
			Some(title)
		} else {
			None
		};
		let song_url = match self.song.engine {
			None if title.is_some() => Some(self.song.song.levelstats_url()),
			_ => None,
		};
		// TODO: let profile_url = ...

		let alt_title = if title.is_some() {
			Some(format!(
				"{} by {} ({})",
				level.name.trim(),
				level.author.trim(),
				level.difficulty
			))
		} else {
			None
		};

		let replay_url = match self.recent_id {
			Some(id) => Some({
				let url = Url::parse_with_params("https://arcnmx.github.io/sweetffr/main/replay.html", [
					("replayid", &format!("100{id}")),
					//("engine", "air"),
					//("skip", "1"),
					//("avatar", &self.song.player.avatar),
				])
				.unwrap();
				if self.ended.is_some() {
					(url, "Replay")
				} else {
					(url, "Last Play")
				}
			}),
			#[cfg(replay_frames)]
			None => {
				use std::fmt::Write;

				let replay = ffr::Replay::R3(ffr::ReplayR3 {
					level: level.level,
					timestamp: {
						let ended = SystemTime::now() - self.ended.map(|ended| Instant::now() - ended).unwrap_or_default();
						ended.duration_since(UNIX_EPOCH).unwrap().as_secs()
					},
					score: format!(
						"{}|{}|{}|{}|{}|{}|{}",
						judge.score(),
						judge.perfect(),
						judge.good,
						judge.average,
						judge.miss,
						judge.boo,
						judge.maxcombo
					),
					frames: {
						if false {
							let mut frames = String::new();
							let mut lastframe = self.start.unwrap();
							for (when, direction) in &self.replay_frames {
								let frametime = when.duration_since(lastframe);
								// TODO: be more accurate than (int) ms!
								let fps = 1000u32 / 30;
								let offset = frametime.as_millis() as u32 / fps;
								lastframe = *when;
								let _ = write!(&mut frames, "{}{:x}", *direction as char, offset as u32);
							}
							frames
						} else {
							String::new()
						}
					},
					settings: if false {
						self.song.player.settings.clone()
					} else {
						ffr::PlayerSettings {
							song_rate: self.song.player.settings.song_rate,
							direction: self.song.player.settings.direction.clone(),
							speed: self.song.player.settings.speed.clone(),
							..Default::default()
						}
					},
					userid: self.song.player.userid,
				});
				let replay = serde_json::to_string(&replay).unwrap();
				let replay_url = Url::parse_with_params("https://arcnmx.github.io/sweetffr/main/replay.html", [
					//("avatar", &self.song.player.avatar),
					("replay", &replay[..]),
					("skip", "1"),
				])
				.unwrap();
				Some((replay_url, "Replay"))
			},
			_ => None,
		};

		move |activity: Activity| {
			let activity = activity.assets(move |assets| match alt_title {
				Some(value) => assets.large_text(value),
				None => assets,
			});
			let activity = match title {
				Some(title) => activity.details(title),
				None => activity,
			};
			let activity = match state {
				Some(state) => activity.state(state),
				None => activity,
			};
			let activity = match timestamp {
				Some((start, end)) => activity.timestamps(|ts| ts.start(start).end(end)),
				None => activity,
			};
			let activity = match song_url {
				Some(url) => activity.append_buttons(move |buttons| buttons.label("Level Stats").url(url)),
				None => activity,
			};
			let activity = match replay_url {
				Some((url, label)) => activity.append_buttons(move |buttons| buttons.label(label).url(url)),
				None => activity,
			};
			// TODO: profile button?
			// TODO: artist button?
			// TODO: level button should go to your own page fuck their broken shit lol
			activity
		}
	}

	pub fn key(&self) -> SongKey {
		song_key(&self.song)
	}
}
