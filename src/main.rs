use {
	self::{
		discord::DiscordClient,
		handler::{event_loop, HandlerAction},
	},
	anyhow::Result,
	clap::Parser,
	futures_util::FutureExt,
	log::{debug, error, info, warn},
	reqwest::Url,
	std::{io, result::Result as StdResult, time::Duration},
	tokio::{pin, select, signal::ctrl_c, task::LocalSet, time::sleep},
	tokio_websockets::ClientBuilder,
};

pub(crate) mod discord;
pub(crate) mod handler;
pub(crate) mod playing;

pub type WebsocketStream = tokio_websockets::WebSocketStream<tokio_websockets::MaybeTlsStream<tokio::net::TcpStream>>;

#[derive(Parser, Debug)]
#[clap(version)]
pub struct Args {
	/// FFR stream overlay websocket URL.
	#[clap(short = 'U', long, default_value = "ws://127.0.0.1:21235")]
	pub url: Url,

	/// Discord API client identifier of the game you're playing.
	///
	/// Use `0` to disable reporting activity to the Discord IPC.
	/// Try `677226551607033903` for "music".
	#[clap(short = 'D', long = "discord-client-id", default_value = "1247041590954950717")]
	pub client_id: Option<u64>,
}

#[tokio::main]
async fn main() -> Result<()> {
	env_logger::init_from_env(
		env_logger::Env::new()
			.filter_or("SWEETFFR_LOG", "warn")
			.write_style_or("SWEETFFR_LOG_STYLE", "always"),
	);

	let args = Args::parse();

	let mut discord = match args.client_id {
		None | Some(0) => None,
		Some(client_id) => Some(DiscordClient::with_client_id(client_id)),
	};

	Ok(loop {
		let local = LocalSet::new();

		let ws = match wait_for_connection(&args.url).await? {
			Some(ws) => ws,
			None => break,
		};

		let main_loop = event_loop(ws, discord.as_mut());
		match local.run_until(main_loop).await? {
			HandlerAction::GameOver =>
				if let Some(discord) = &mut discord {
					if let Err(e) = discord.shutdown().await {
						error!("shutdown failed: {e}");
					}
				},
			HandlerAction::Exit => break,
		}
	})
}

async fn connect(url: &Url) -> StdResult<WebsocketStream, tokio_websockets::Error> {
	let (client, response) = ClientBuilder::new()
		.uri(url.as_str())
		.map_err(|e| tokio_websockets::Error::Io(io::Error::new(io::ErrorKind::InvalidData, e)))?
		.connect()
		.await?;
	debug!("Websocket response: {response:#?}");
	info!("Websocket connected");
	Ok(client)
}

async fn connect_retry(url: &Url, count: usize) -> Result<Option<WebsocketStream>> {
	match connect(url).await {
		Err(tokio_websockets::Error::Io(e)) if e.kind() == io::ErrorKind::ConnectionRefused => {
			if count == 0 {
				warn!("Websocket IO error: {e:}");
				info!("Waiting for websocket connection...");
			}
			sleep(Duration::from_secs(5)).await;
			Ok(None)
		},
		res => res.map(Some).map_err(Into::into),
	}
}

async fn wait_for_connection(url: &Url) -> Result<Option<WebsocketStream>> {
	let interrupt = ctrl_c().fuse();
	pin!(interrupt);
	let mut retries = 0usize;

	loop {
		let websocket = connect_retry(url, retries);
		select! {
			res = &mut interrupt => match res {
				Ok(()) => {
					debug!("^C interrupt received, exiting...");
					return Ok(None)
				},
				Err(e) => {
					error!("^C signal error: {e}");
				},
			},
			res = websocket => match res {
				Ok(Some(ws)) => return Ok(Some(ws)),
				Ok(None) => retries = retries.saturating_add(1),
				Err(e) => {
					error!("Websocket error: {e}");
					return Err(e)
				},
			},
		}
	}
}
