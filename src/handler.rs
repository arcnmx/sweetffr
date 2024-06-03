use {
	crate::{
		discord::{DiscordClient, DiscordEvent},
		playing::{NowPlaying, Playing, SongKey},
		WebsocketStream,
	},
	anyhow::Result,
	futures_util::{
		future::{self, Either},
		poll, FutureExt, SinkExt, StreamExt,
	},
	log::{debug, error, info},
	std::{task::Poll, time::Duration},
	sweetffr::ffr::{
		stream::{StreamEvent, StreamMessage, StreamSong},
		Judge,
	},
	tokio::{
		pin, select,
		signal::ctrl_c,
		spawn,
		sync::{mpsc as channel, Mutex},
		time::{timeout, Instant},
	},
};

#[derive(Debug)]
enum HandlerEvent {
	RecentReplay { key: SongKey, recent_id: u32 },
}

#[derive(Debug)]
pub enum HandlerAction {
	GameOver,
	Exit,
}

#[derive(Debug)]
struct Handler {
	playing: Playing,
	handler_tx: channel::Sender<HandlerEvent>,
	last_update: Mutex<Option<Instant>>,
}

impl Handler {
	// or 5 or 10 or 15?
	pub const DISCORD_UPDATE_THRESHOLD: Duration = Duration::from_secs(3);

	pub fn new(handler_tx: channel::Sender<HandlerEvent>) -> Self {
		Self {
			playing: Default::default(),
			handler_tx,
			last_update: Default::default(),
		}
	}

	async fn discord_update(
		&self,
		important: bool,
		play: Option<&NowPlaying>,
		discord: &mut Option<&mut DiscordClient>,
	) -> Result<()> {
		match (important, &*self.last_update.lock().await) {
			(false, &Some(last_update)) if Instant::now().duration_since(last_update) < Self::DISCORD_UPDATE_THRESHOLD => {
				// throttle unnecessary updates
				return Ok(());
			},
			_ => (),
		}
		*self.last_update.lock().await = Some(Instant::now());

		let activity = self.playing.discord_activity(play);
		debug!("Activity update: {activity:#?}");
		if let Some(discord) = discord {
			let max = match important {
				false => Duration::from_millis(50),
				true => Duration::from_secs(1),
			};
			let send = discord.send(Some(activity));
			match timeout(max, send).await? {
				Ok(()) => {
					*self.last_update.lock().await = Some(Instant::now());
				},
				Err(e) => {
					debug!("DiscordIPC(activity) update: {e}");
				},
			}
		}
		Ok(())
	}

	async fn handle_event(
		&mut self,
		event: StreamMessage,
		mut discord: Option<&mut DiscordClient>,
	) -> Result<Option<HandlerAction>> {
		self.playing.process_event(&event);
		let play = match event.event.song() {
			Some(song) => self.playing.now_playing(song),
			None => self.playing.latest(),
		};
		if let Some(play) = play {
			let important = match event.event {
				StreamEvent::NoteJudge(..) => false,
				_ => true,
			};
			self.discord_update(important, Some(play), &mut discord).await?;

			match &event.event {
				#[cfg(feature = "recent")]
				StreamEvent::SongEnd(..) if play.judge.score() > 0 => {
					use {log::warn, tokio::time::sleep};

					let key = play.key();
					let song = play.song.clone();
					let judge = play.judge.judge.clone();
					let handler_tx = self.handler_tx.clone();
					spawn(async move {
						const TARGET: &'static str = "sweetffr::handler::recent";
						sleep(Duration::from_secs(2)).await;
						let max = Duration::from_secs(10);
						match timeout(max, Self::find_replay(&song, &judge)).await {
							Err(e) => {
								warn!(target: TARGET, "fetch timed out: {e}");
							},
							Ok(Err(e)) => {
								error!(target: TARGET, "{e}");
							},
							Ok(Ok(None)) => {
								info!(target: TARGET, "recent replay not saved");
							},
							Ok(Ok(Some(recent_id))) => {
								let res = handler_tx.send(HandlerEvent::RecentReplay { key, recent_id }).await;
								if let Err(e) = res {
									debug!(target: TARGET, "Handler not listening for {e:?}");
								}
							},
						}
					});
				},
				_ => (),
			}
		}
		Ok(None)
	}

	async fn handle(
		&mut self,
		event: HandlerEvent,
		mut discord: Option<&mut DiscordClient>,
	) -> Result<Option<HandlerAction>> {
		match &event {
			HandlerEvent::RecentReplay { key, recent_id } => {
				if let Some(play) = self.playing.now_playing.get_mut(key) {
					play.recent(*recent_id);
				};
				self.discord_update(true, None, &mut discord).await?;
			},
		}

		Ok(None)
	}

	/// Scrape replay off homepage...
	#[cfg(feature = "recent")]
	async fn find_replay(song: &StreamSong, judge: &Judge) -> Result<Option<u32>> {
		use sweetffr::ffr::recent::RecentIndex;

		let request = reqwest::get("https://www.flashflashrevolution.com/arc/recent.php");
		let recent = request.await?;
		let document = recent.bytes().await?;
		let recents = RecentIndex::parse_document_bytes(&document);
		let recent = recents.into_iter().find(|recent| {
			recent.username.as_ref() == Some(&song.player.name)
			//&& recent.song_name.as_ref() == Some(&song.song.name)
			&& recent.score == judge.score()
		});
		Ok(recent.and_then(|recent| recent.recent_id()))
	}
}

pub async fn event_loop(mut ws: WebsocketStream, mut discord: Option<&mut DiscordClient>) -> Result<HandlerAction> {
	if let Some(discord) = &mut discord {
		discord.connect().await?;
	}

	let (handler_tx, mut handler_rx) = channel::channel(8);
	let mut handler = Handler::new(handler_tx);

	handler.discord_update(true, None, &mut discord).await?;

	let interrupt = ctrl_c().fuse();
	pin!(interrupt);

	let action = loop {
		let discord_event = discord
			.as_mut()
			.map(|discord| Either::Left(discord.next()))
			.unwrap_or(Either::Right(future::pending()));
		select! {
			res = &mut interrupt => match res {
				Ok(()) => {
					debug!("^C interrupt received, exiting...");
					break HandlerAction::Exit;
				},
				Err(e) => {
					error!("^C signal error: {e}");
				},
			},
			event = ws.next() => match event {
				None => {
					info!("Websocket connection closed, game over...");
					break HandlerAction::GameOver;
				},
				Some(Err(e)) => return Err(e.into()),
				Some(Ok(msg)) => match msg.as_text() {
					Some(json) => {
						debug!("StreamMessage(JSON): {json}");
						let event = match serde_json::from_str::<StreamMessage>(json) {
							Ok(event) => event,
							Err(e) => {
								error!("invalid StreamMessage: {e}");
								continue
							},
						};
						let handle_event = handler.handle_event(event, discord.as_mut().map(|d| &mut **d));
						match handle_event.await? {
							Some(action) => break action,
							None => (),
						};
					},
					None => {
						debug!("Websocket message empty: {msg:?}");
					},
				},
			},
			Some(event) = handler_rx.recv() => {
				handler.handle(event, discord.as_mut().map(|d| &mut **d)).await?;
			},
			event = discord_event => match event {
				None => {
					info!("Discord event stream eof");
				},
				Some(event@DiscordEvent::Ready(..)) => {
					debug!("Discord ready: {event:?}");
					info!("Discord ready, updating...");
					handler.discord_update(true, None, &mut discord).await?;
				},
				Some(event) => {
					debug!("Discord event ignored: {event:?}");
				},
			},
		}
	};

	if let Some(discord) = &mut discord {
		let mut activity_tx = discord.activity_sender().clone();
		let mut clear_activity = Box::pin(async move {
			if let Err(e) = activity_tx.send(None).await {
				debug!("Failed to clear activity on game exit: {e:?}");
			}
		});
		if let Poll::Pending = poll!(&mut clear_activity) {
			spawn(clear_activity);
		}
	}

	Ok(action)
}
