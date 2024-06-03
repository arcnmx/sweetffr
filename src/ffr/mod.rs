pub use self::judge::Judge;
use {
	serde::{Deserialize, Serialize},
	serde_json::{Map, Value},
	std::time::Duration,
};

mod judge;
#[cfg(feature = "recent")]
pub mod recent;
pub mod replay;
pub mod stream;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(debug_assertions, serde(deny_unknown_fields))]
pub struct Player {
	pub userid: i64,
	pub name: String,
	pub avatar: String,
	pub game_grand_total: i64,
	pub game_played: i64,
	pub game_rank: i64,
	pub skill_level: i64,
	pub skill_rating: f64,
	pub settings: PlayerSettings,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PlayerSettings {
	pub settings: Map<String, Value>,
}

macro_rules! player_setting {
	(fn $id:ident -> $ty:ty;) => {
		player_setting! { fn $id@(stringify!($id)) -> $ty; }
	};
	(fn $id:ident@($key:expr) -> $ty:ty;) => {
		pub fn $id(&self) -> serde_json::Result<Option<$ty>> {
			self.settings.get($key).map(Deserialize::deserialize).transpose()
		}
	};
}
impl PlayerSettings {
	player_setting! { fn direction -> String; }
	player_setting! { fn speed -> f64; }
	player_setting! { fn judge_colors@("judgeColors") -> [u64; 6]; }

	pub fn rate(&self) -> serde_json::Result<Option<f64>> {
		let rate = self
			.settings
			.get("songRate")
			.map(Deserialize::deserialize)
			.transpose()?;
		Ok(match rate {
			Some(rate) if rate == 1.0 => None,
			rate => rate,
		})
	}
}

#[derive(Debug, Clone, Default, PartialOrd, PartialEq, Serialize, Deserialize)]
#[cfg_attr(debug_assertions, serde(deny_unknown_fields))]
pub struct Level {
	pub genre: i64,
	pub level: u32,
	pub name: String,
	pub author: String,
	#[serde(default)]
	pub author_url: Option<String>,
	pub stepauthor: String,
	pub style: String,
	pub difficulty: u32,
	pub note_count: u32,
	pub time: String,
	pub time_seconds: f64,
	pub credits: i64,
	#[serde(default)]
	pub release_date: Option<u64>,
	pub nps_min: f64,
	pub nps_avg: f64,
	pub nps_max: f64,
	pub song_rating: Option<f64>,
}

impl Level {
	pub fn duration(&self) -> Duration {
		Duration::from_secs_f64(self.time_seconds)
	}

	pub fn levelstats_url(&self) -> String {
		format!(
			"https://www.flashflashrevolution.com/levelstats.php?level={}",
			self.level
		)
	}
}

#[derive(Debug, Clone, Default, PartialOrd, PartialEq, Serialize, Deserialize)]
#[cfg_attr(debug_assertions, serde(deny_unknown_fields))]
pub struct BestScore {
	#[serde(flatten)]
	pub judge: Judge,
	pub genre: i64,
	pub rank: i64,
	pub rawscore: i64,
	pub results: String,
	pub fcs: u64,
	pub plays: u64,
	pub aaas: u64,
	pub equiv: f64,
	pub id: i64,
}

impl BestScore {
	pub fn is_unplayed(&self) -> bool {
		self.plays == 0
	}
}
