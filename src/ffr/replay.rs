use {
	crate::ffr::PlayerSettings,
	serde::{Deserialize, Serialize},
};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(debug_assertions, serde(deny_unknown_fields))]
#[serde(tag = "replayversion")]
pub enum Replay {
	#[serde(rename = "R^3")]
	R3(ReplayR3),
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(debug_assertions, serde(deny_unknown_fields))]
pub struct ReplayR3 {
	#[serde(rename = "replaylevelid")]
	pub level: u32,
	pub timestamp: u64,
	#[serde(rename = "replayframes")]
	pub frames: String,
	#[serde(rename = "replayscore")]
	pub score: String,
	#[serde(rename = "replaysettings")]
	pub settings: PlayerSettings,
	pub userid: i64,
}
