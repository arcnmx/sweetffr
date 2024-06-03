//! [Websocket stream overlay protocol](https://github.com/flashflashrevolution/web-stream-overlay)

use {
	crate::ffr::{BestScore, Judge, Level, Player},
	serde::{Deserialize, Serialize},
	std::{
		ops::{Deref, DerefMut},
		time::Duration,
	},
};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(debug_assertions, serde(deny_unknown_fields))]
pub struct StreamMessage {
	#[serde(rename = "senderId")]
	pub sender_id: i32,
	#[serde(flatten)]
	pub event: StreamEvent,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "command", content = "data", rename_all = "SCREAMING_SNAKE_CASE")]
#[cfg_attr(debug_assertions, serde(deny_unknown_fields))]
pub enum StreamEvent {
	SongStart(StreamSong),
	SongPause(StreamSong),
	SongResume(StreamSong),
	SongEnd(StreamSong),
	SongRestart(StreamSong),
	NoteJudge(StreamJudge),
}

impl StreamEvent {
	pub fn song(&self) -> Option<&StreamSong> {
		match self {
			Self::SongStart(s) | Self::SongPause(s) | Self::SongResume(s) | Self::SongEnd(s) | Self::SongRestart(s) =>
				Some(s),
			Self::NoteJudge(..) => None,
		}
	}
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(debug_assertions, serde(deny_unknown_fields))]
pub struct StreamSong {
	pub player: Player,
	pub song: Level,
	#[serde(default)]
	pub engine: Option<StreamEngine>,
	pub best_score: BestScore,
}

#[derive(Debug, Clone, Default, PartialOrd, PartialEq, Serialize, Deserialize)]
#[cfg_attr(debug_assertions, serde(deny_unknown_fields))]
pub struct StreamEngine {
	pub domain: String,
	pub config: String,
	pub id: String,
	pub name: String,
}

#[derive(Debug, Clone, Default, PartialOrd, PartialEq, Serialize, Deserialize)]
#[cfg_attr(debug_assertions, serde(deny_unknown_fields))]
pub struct StreamJudge {
	#[serde(flatten)]
	pub judge: Judge,
	pub combo: i32,
	pub restarts: i32,
	pub last_hit: Option<i32>,
}

impl Deref for StreamJudge {
	type Target = Judge;

	fn deref(&self) -> &Self::Target {
		&self.judge
	}
}

impl DerefMut for StreamJudge {
	fn deref_mut(&mut self) -> &mut Self::Target {
		&mut self.judge
	}
}
