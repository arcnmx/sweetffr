use {
	crate::ffr::Level,
	serde::{Deserialize, Serialize},
	std::cmp::Ordering,
};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(debug_assertions, serde(deny_unknown_fields))]
pub struct Judge {
	#[serde(default)]
	pub amazing: Option<u32>,
	pub perfect: u32,
	pub good: u32,
	pub average: u32,
	pub miss: u32,
	pub boo: u32,
	#[serde(default)]
	pub score: Option<i32>,
	pub maxcombo: u32,
}

impl Judge {
	pub const ALPHA: f64 = 9.9750396740034;
	pub const BETA: f64 = 0.0193296437339205;
	pub const LAMBDA: f64 = 18206628.7286425;

	pub const D1: f64 = 17678803623.9633;
	pub const D2: f64 = 733763392.922176;
	pub const D3: f64 = 28163834.4879901;
	pub const D4: f64 = -434698.513947563;
	pub const D5: f64 = 3060.24243867853;

	pub fn aaa(note_count: u32) -> Self {
		Self {
			perfect: note_count,
			maxcombo: note_count,
			..Self::default()
		}
	}

	pub fn aaaa(note_count: u32) -> Self {
		Self {
			amazing: Some(note_count),
			maxcombo: note_count,
			..Self::default()
		}
	}

	pub fn perfect(&self) -> u32 {
		self.perfect + self.amazing.unwrap_or(0)
	}

	pub fn score(&self) -> i32 {
		let score = self.perfect() * 550 + self.good * 275 + self.average * 55;
		score as i32 - self.miss as i32 * 310 - self.boo as i32 * 20 + self.maxcombo as i32 * 1000
	}

	pub fn raw_score(&self) -> i32 {
		match self.score {
			Some(score) => score,
			None => {
				let score = self.perfect() * 50 + self.good * 25 + self.average * 5;
				score as i32 - self.miss as i32 * 10 - self.boo as i32 * 5
			},
		}
	}

	pub fn raw_goods(&self) -> f64 {
		self.good as f64 + self.average as f64 * 1.8f64 + self.miss as f64 * 2.4f64 + self.boo as f64 * 0.2f64
	}

	/// Song Weight
	pub fn aaa_equivalency(&self, difficulty: u32) -> f64 {
		let raw_goods = self.raw_goods();
		let difficulty = difficulty as f64;
		let delta = Self::D1
			+ Self::D2 * difficulty as f64
			+ Self::D3 * difficulty.powi(2)
			+ Self::D4 * difficulty.powi(3)
			+ Self::D5 * difficulty.powi(4);
		if delta - raw_goods * Self::LAMBDA > 0.0 {
			let value = (delta - raw_goods * Self::LAMBDA) / delta * (difficulty + Self::ALPHA).powf(Self::BETA);
			value.powf(1.0 / Self::BETA) - Self::ALPHA
		} else {
			0.0
		}
		.max(0.0)
	}

	pub fn results(&self) -> String {
		let &Self {
			good,
			average,
			miss,
			boo,
			maxcombo,
			..
		} = self;
		let perfect = self.perfect();
		format!("{perfect}-{good}-{average}-{miss}-{boo}-{maxcombo}")
	}

	pub fn note_count(&self) -> u32 {
		self.hit_count() + self.miss
	}

	pub fn hit_count(&self) -> u32 {
		self.perfect() + self.good + self.average
	}

	pub fn is_complete(&self, level: &Level) -> bool {
		self.note_count() >= level.note_count
	}

	pub fn is_full_combo(&self) -> bool {
		self.miss == 0
		//self.hit_count() == self.maxcombo
	}

	pub fn is_aaaa(&self) -> bool {
		self.is_aaa() && self.perfect == 0
	}

	pub fn is_aaa(&self) -> bool {
		self.is_full_combo() && self.good == 0 && self.average == 0 && self.miss == 0 && self.boo == 0
	}
}

impl PartialOrd for Judge {
	fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
		Some(self.cmp(other))
	}
}

impl Ord for Judge {
	fn cmp(&self, other: &Self) -> Ordering {
		self
			.raw_score()
			.cmp(&other.raw_score())
			.then_with(|| {
				self
					.perfect()
					.cmp(&other.perfect())
					.then(self.amazing.cmp(&other.amazing))
			})
			.then_with(|| self.good.cmp(&other.good))
			.then_with(|| self.average.cmp(&other.average))
			.then_with(|| self.miss.cmp(&other.miss).reverse())
			.then_with(|| self.boo.cmp(&other.boo).reverse())
			.then_with(|| self.maxcombo.cmp(&other.maxcombo).reverse())
			.then_with(|| self.score().cmp(&other.score()))
	}
}
