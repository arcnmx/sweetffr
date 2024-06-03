use {
	html5ever::{
		interface::{NodeOrText, QuirksMode, TreeSink},
		parse_document,
		tendril::{stream::Utf8LossyDecoder, StrTendril, TendrilSink},
		Attribute, ParseOpts, Parser, QualName,
	},
	std::{borrow::Cow, collections::HashMap},
};

#[test]
fn recent_parse() {
	let input = include_bytes!("../../data/recent.php");
	let recents = RecentIndex::parse_document_bytes(input);

	eprintln!("{recents:#?}");

	for recent in recents {
		eprintln!("{:?}", recent.recent_id());
	}
}

#[derive(Debug, Clone, Default)]
pub struct RecentScore {
	pub profile_href: Option<String>,
	pub avatar_src: Option<String>,
	pub username: Option<String>,
	pub rank: u32,
	pub recent_href: Option<String>,
	pub results: Option<String>,
	pub song_name: Option<String>,
	pub max_combo: u32,
	pub score: i32,
	pub date: Option<String>,
}

impl RecentScore {
	pub fn recent_id(&self) -> Option<u32> {
		let recent = self.recent_href.as_ref()?;

		let id = recent.get(13..)?.parse().expect("numeric recent id");

		Some(id)
	}
}

impl RecentIndex {
	pub fn parse_document() -> Utf8LossyDecoder<Parser<Self>> {
		let opts = ParseOpts::default();
		parse_document(Self::new(), opts).from_utf8()
	}

	pub fn parse_document_bytes(document: &[u8]) -> Vec<RecentScore> {
		Self::parse_document().one(document)
	}
}

#[derive(Debug, Clone)]
pub struct RecentIndex {
	next_id: usize,
	names: HashMap<usize, QualName>,
	data: Option<(RecentScore, usize)>,
	plays: Option<Vec<RecentScore>>,
}

impl RecentIndex {
	pub fn new() -> Self {
		Self {
			next_id: 1,
			names: Default::default(),
			data: None,
			plays: None,
		}
	}

	fn get_id(&mut self) -> usize {
		let id = self.next_id;
		self.next_id += 2;
		id
	}
}

#[allow(unused_variables)]
impl TreeSink for RecentIndex {
	type Handle = usize;
	type Output = Vec<RecentScore>;

	fn finish(mut self) -> Self::Output {
		let mut plays = self.plays.take().expect("did not recognize document");

		if let Some((data, state)) = self.data.take() {
			assert_eq!(state, 9);
			plays.push(data);
		}

		plays
	}

	fn get_document(&mut self) -> Self::Handle {
		0
	}

	fn get_template_contents(&mut self, target: &Self::Handle) -> Self::Handle {
		unimplemented!()
	}

	fn same_node(&self, x: &Self::Handle, y: &Self::Handle) -> bool {
		x == y
	}

	fn elem_name<'a>(&'a self, target: &'a Self::Handle) -> html5ever::ExpandedName<'a> {
		self.names.get(target).expect("not an element").expanded()
	}

	fn create_element(
		&mut self,
		name: QualName,
		attrs: Vec<html5ever::Attribute>,
		flags: html5ever::interface::ElementFlags,
	) -> Self::Handle {
		let id = self.get_id();
		// table fields: Avatar, Player, Rank, Song, Results, Max Combo, Score, Date

		if &name.local == "h2"
			&& attrs
				.iter()
				.any(|a| &a.name.local == "class" && &a.value[..] == "center")
		{
			// XXX: content should == "Recently Played Songs"
			self.plays = Some(Vec::new());
		} else if &name.local == "tr"
			&& attrs
				.iter()
				.any(|a| &a.name.local == "class" && a.value.starts_with("zebra"))
		{
			if let Some((data, state)) = self.data.replace((RecentScore::default(), 0)) {
				assert_eq!(state, 9);
				let plays = self.plays.as_mut().expect("seen header table");
				plays.push(data);
			}
		} else if &name.local == "td" {
			match self.data.as_mut() {
				Some((data, state)) => {
					*state += 1;
				},
				None => (),
			}
		} else if &name.local == "a" {
			match self.data.as_mut() {
				Some((data, 1 | 2)) => {
					let href = attrs
						.iter()
						.find(|a| &a.name.local == "href")
						.map(|a| a.value.to_string());

					if let (Some(have), Some(found)) = (&data.profile_href, &href) {
						assert_eq!(have, found);
					}
					if href.is_some() {
						data.profile_href = href;
					}
				},
				Some((data, 4)) => {
					let href = attrs
						.iter()
						.find(|a| &a.name.local == "href")
						.map(|a| a.value.to_string());

					data.recent_href = href;
				},
				_ => (),
			}
		} else if &name.local == "img" {
			match self.data.as_mut() {
				Some((data, 1)) => {
					data.avatar_src = attrs
						.iter()
						.find(|a| &a.name.local == "src")
						.map(|a| a.value.to_string());
				},
				_ => (),
			}
		}

		self.names.insert(id, name);
		id
	}

	fn create_comment(&mut self, _text: StrTendril) -> usize {
		self.get_id()
	}

	fn create_pi(&mut self, target: StrTendril, value: StrTendril) -> usize {
		unimplemented!()
	}

	fn append_before_sibling(&mut self, _sibling: &usize, _new_node: NodeOrText<usize>) {}

	fn append_based_on_parent_node(&mut self, _element: &usize, _prev_element: &usize, _new_node: NodeOrText<usize>) {}

	fn parse_error(&mut self, _msg: Cow<'static, str>) {}
	fn set_quirks_mode(&mut self, _mode: QuirksMode) {}
	fn append(&mut self, _parent: &usize, child: NodeOrText<usize>) {
		if let NodeOrText::AppendText(text) = child {
			match self.data.as_mut() {
				Some((data, 2)) if !text.trim().is_empty() => {
					data.username = Some(text.trim().into());
				},
				Some((data, 3)) if !text.trim().is_empty() => {
					data.rank = text.trim().replace(",", "").parse().expect("numeric rank");
				},
				Some((data, 4)) if !text.trim().is_empty() => {
					data.song_name = Some(text.trim().into());
				},
				Some((data, 5)) if !text.trim().is_empty() => {
					data.results = Some(text.trim().into());
				},
				Some((data, 6)) if !text.trim().is_empty() => {
					data.max_combo = text.trim().replace(",", "").parse().expect("numeric combo");
				},
				Some((data, 7)) if !text.trim().is_empty() => {
					data.score = text.trim().replace(",", "").parse().expect("numeric score");
				},
				Some((data, state @ 8)) if !text.trim().is_empty() => {
					data.date = Some(text.trim().into());
					*state += 1;
				},
				_ => (),
			}
		}
	}

	fn append_doctype_to_document(&mut self, _: StrTendril, _: StrTendril, _: StrTendril) {}
	fn add_attrs_if_missing(&mut self, target: &usize, _attrs: Vec<Attribute>) {
		assert!(self.names.contains_key(target), "not an element");
	}
	fn remove_from_parent(&mut self, _target: &usize) {}
	fn reparent_children(&mut self, _node: &usize, _new_parent: &usize) {}
	fn mark_script_already_started(&mut self, _node: &usize) {}
}
