pub use discord_presence::models::EventData as DiscordEvent;
use {
	anyhow::{Error, Result},
	discord_presence::{models::Activity, Client},
	futures_util::{FutureExt, Sink, SinkExt, Stream},
	log::{debug, error, info, trace, warn},
	std::{
		future::Future,
		mem::replace,
		pin::Pin,
		sync::Arc,
		task::{Context, Poll},
		time::Duration,
	},
	tokio::{
		spawn,
		sync::{mpsc as channel, Mutex},
		task::{spawn_blocking, JoinHandle},
		time::timeout,
	},
	tokio_util::sync::PollSender,
};

#[allow(dyn_drop)]
pub type DiscordEventHandler = Box<dyn Drop>;

pub struct DiscordState {
	tx: channel::Sender<DiscordEvent>,
	active: bool,
	event_handlers: Vec<DiscordEventHandler>,
}

impl DiscordState {
	pub fn new(tx: channel::Sender<DiscordEvent>) -> Self {
		Self {
			tx,
			active: false,
			event_handlers: Default::default(),
		}
	}

	pub fn event_state(&self) -> DiscordEventState {
		DiscordEventState { tx: self.tx.clone() }
	}

	pub fn setup_event_handlers(&mut self, client: &Client) {
		const TARGET: &'static str = "sweetffr::discord::event";

		let ready = client.on_ready({
			let state = self.event_state();
			move |event| {
				info!(target: TARGET, "{event:?}");
				state.blocking_send(event.event);
			}
		});
		let error = client.on_error({
			let state = self.event_state();
			move |event| {
				error!(target: TARGET, "{event:?}");
				state.blocking_send(event.event);
			}
		});
		let activity_spectate = client.on_activity_spectate({
			let state = self.event_state();
			move |event| {
				info!(target: TARGET, "{event:?}");
				state.blocking_send(event.event);
			}
		});
		let activity_join = client.on_activity_join({
			let state = self.event_state();
			move |event| {
				info!(target: TARGET, "{event:?}");
				state.blocking_send(event.event);
			}
		});
		let activity_join_request = client.on_activity_join_request({
			let state = self.event_state();
			move |event| {
				info!(target: TARGET, "{event:?}");
				state.blocking_send(event.event);
			}
		});
		self.event_handlers.extend([
			Box::new(ready) as DiscordEventHandler,
			Box::new(error) as DiscordEventHandler,
			Box::new(activity_spectate) as DiscordEventHandler,
			Box::new(activity_join) as DiscordEventHandler,
			Box::new(activity_join_request) as DiscordEventHandler,
		]);
	}

	pub fn take_handlers(&mut self) -> Vec<DiscordEventHandler> {
		self.active = false;
		replace(&mut self.event_handlers, Default::default())
	}
}

pub struct DiscordClient {
	pub client_id: u64,
	pub client: Arc<Mutex<Client>>,
	rx: channel::Receiver<DiscordEvent>,
	activity_tx: PollSender<Option<Activity>>,
	activity_task: JoinHandle<()>,
	state: Arc<Mutex<DiscordState>>,
}

#[derive(Debug, Clone)]
pub struct DiscordEventState {
	tx: channel::Sender<DiscordEvent>,
}

impl DiscordEventState {
	fn blocking_send(&self, event: DiscordEvent) {
		if let Err(e) = self.tx.blocking_send(event) {
			debug!("couldn't send event: {e:?}");
		}
	}
}

impl DiscordClient {
	pub fn with_client_id(client_id: u64) -> Self {
		let client = Self::new_client(client_id);
		Self::new(client, client_id)
	}

	fn new(client: Client, client_id: u64) -> Self {
		let (tx, rx) = channel::channel(0x4);
		let (activity_tx, activity_rx) = channel::channel(4);
		let mut state = DiscordState::new(tx);
		state.setup_event_handlers(&client);
		let client = Arc::new(Mutex::new(client));
		Self {
			activity_task: spawn(Self::activity_main(activity_rx, &client)),
			client,
			client_id,
			rx,
			activity_tx: PollSender::new(activity_tx),
			state: Arc::new(Mutex::new(state)),
		}
	}

	fn new_client(client_id: u64) -> Client {
		let sleep_duration = Duration::from_secs(1);
		let attempts = None;
		Client::with_error_config(client_id, sleep_duration, attempts)
	}

	async fn recv_last<T>(rx: &mut channel::Receiver<T>) -> Option<T> {
		let mut buf = Vec::with_capacity(rx.len());
		let _amt = rx.recv_many(&mut buf, rx.max_capacity()).await;
		buf.into_iter().last()
	}

	fn activity_main(
		mut rx: channel::Receiver<Option<Activity>>,
		client: &'_ Arc<Mutex<Client>>,
	) -> impl Future<Output = ()> {
		let client = Arc::downgrade(client);
		const TARGET: &'static str = "sweetffr::discord::activity_main";
		async move {
			while let Some(activity) = Self::recv_last(&mut rx).await {
				trace!(target: TARGET, "activity received: {activity:?}");
				if !Client::is_ready() {
					debug!(target: TARGET, "not ready for {activity:?}");
					continue
				}
				let client = match client.upgrade() {
					Some(c) => c,
					None => {
						info!(target: TARGET, "client no longer referenced");
						break
					},
				};
				let update = async move {
					let mut client = client.lock_owned().await;
					let update = move || {
						match activity {
							Some(activity) => client.set_activity(|_| activity),
							None => client.clear_activity(),
						}
						.map(drop)
					};
					spawn_blocking(update)
						.await
						.map_err(Error::from)
						.and_then(|res| res.map_err(Error::from))
				};
				match timeout(Duration::from_secs(1), update).await {
					Err(e) => warn!(target: TARGET, "{e}"),
					Ok(Err(e)) => error!(target: TARGET, "set failed: {e}"),
					Ok(Ok(())) => (),
				}
			}
			info!(target: TARGET, "exited upon eof");
		}
	}

	async fn take_client(&mut self) -> (Client, Vec<DiscordEventHandler>) {
		let client = Self::new_client(self.client_id);
		let mut state = self.state.lock().await;
		let handlers = state.take_handlers();
		state.setup_event_handlers(&client);
		drop(state);
		let old_client = replace(&mut *self.client.lock().await, client);
		(old_client, handlers)
	}

	pub fn is_active(&self) -> bool {
		Client::is_ready() || self.state.try_lock().unwrap().active
	}

	pub async fn connect(&mut self) -> Result<()> {
		if let Err(e) = self.shutdown().await {
			warn!("shutdown failed: {e:?}");
		}
		info!("connecting...");
		let client = self.client.clone();
		let res = spawn_blocking(move || client.blocking_lock().start()).await?;
		Ok(res)
	}

	pub async fn shutdown(&mut self) -> Result<bool> {
		let is_active = self.is_active();
		if is_active {
			info!("shutting down...");
			let (client, handlers) = self.take_client().await;
			spawn_blocking(move || client.shutdown()).await??;
			drop(handlers);
			Ok(true)
		} else {
			Ok(false)
		}
	}

	pub fn activity_sender(&self) -> &PollSender<Option<Activity>> {
		&self.activity_tx
	}
}

impl Stream for DiscordClient {
	type Item = DiscordEvent;

	fn poll_next(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
		Pin::into_inner(self).rx.poll_recv(cx)
	}

	fn size_hint(&self) -> (usize, Option<usize>) {
		(self.rx.len(), None)
	}
}

impl Sink<Option<Activity>> for DiscordClient {
	type Error = Error;

	fn start_send(self: Pin<&mut Self>, item: Option<Activity>) -> Result<(), Self::Error> {
		Pin::into_inner(self).activity_tx.send_item(item).map_err(Into::into)
	}

	fn poll_ready(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Result<(), Self::Error>> {
		Pin::into_inner(self).activity_tx.poll_reserve(cx).map_err(Into::into)
	}

	fn poll_flush(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Result<(), Self::Error>> {
		// XXX: this impl doesn't actually do anything...
		// this could `abort_send()` and `reserve_many(sender.max_capacity())` to function as expected,
		// but... it would fight with other senders in that case?
		Pin::into_inner(self)
			.activity_tx
			.poll_flush_unpin(cx)
			.map_err(Into::into)
	}

	fn poll_close(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Result<(), Self::Error>> {
		let this = Pin::into_inner(self);
		if !this.activity_tx.is_closed() {
			this.activity_tx.close();
		}
		this.activity_task.poll_unpin(cx).map_err(Into::into)
		// TODO: client.shutdown once Poll::Ready?
	}
}
