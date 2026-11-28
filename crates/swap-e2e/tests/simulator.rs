//! The real Twilight transport against the local Discord simulator.

use std::collections::BTreeMap;
use std::time::Duration;

use base64::Engine as _;
use swap_discord::{
    AttachmentFetcher, DiscordGuildId, DiscordPort, DiscordUserId, GatewayConnector,
    HttpAttachmentFetcher, HttpDiscordPort, InboundEvent, OutboundFile, OutboundMessage,
    SetupDirectory, TwilightConfig, TwilightConnector, TwilightSetup,
};
use swap_e2e::{DiscordSim, InjectMessageRequest, mod_track};

fn timeout() -> Duration {
    Duration::from_secs(10)
}

// scenario: The bot registers the guild command
#[tokio::test(flavor = "multi_thread")]
async fn the_gateway_and_http_round_trip() {
    let sim = DiscordSim::start().await.unwrap();
    let connector = TwilightConnector::with_proxy(
        TwilightConfig::new(sim.token(), DiscordGuildId::new(sim.guild_id())),
        sim.http_host(),
    )
    .unwrap();
    let mut session = connector.connect().await.unwrap();
    tokio::time::timeout(timeout(), session.wait_ready())
        .await
        .expect("the gateway must become ready")
        .unwrap();
    assert_eq!(connector.register_commands().await.unwrap(), 1);
    assert!(sim.wait_commands(timeout()).await);
    assert_eq!(sim.command_count(), 1);

    let port = HttpDiscordPort::with_proxy(sim.token(), sim.http_host()).unwrap();
    let mira = DiscordUserId::new(sim.participant("mira").unwrap());

    port.send_dm(
        &mira,
        OutboundMessage {
            content: "Welcome to the swap.".to_string(),
        },
    )
    .await
    .unwrap();
    assert!(
        sim.last_bot_message(&mira.0)
            .unwrap()
            .contains("Welcome to the swap.")
    );

    let module = mod_track("Night drive", 0, 0);
    port.send_file(
        &mira,
        OutboundFile {
            filename: "seed.mod".to_string(),
            caption: "Your pack.".to_string(),
            bytes: module.clone(),
        },
    )
    .await
    .unwrap();
    let files = sim.bot_attachments_for(&mira.0);
    assert_eq!(files.len(), 1);
    assert_eq!(files[0].0, "seed.mod");
    assert_eq!(files[0].1, module);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_participant_upload_arrives_as_a_dm_attachment() {
    let sim = DiscordSim::start().await.unwrap();
    let connector = TwilightConnector::with_proxy(
        TwilightConfig::new(sim.token(), DiscordGuildId::new(sim.guild_id())),
        sim.http_host(),
    )
    .unwrap();
    let mut session = connector.connect().await.unwrap();
    tokio::time::timeout(timeout(), session.wait_ready())
        .await
        .expect("the gateway must become ready")
        .unwrap();

    let port = HttpDiscordPort::with_proxy(sim.token(), sim.http_host()).unwrap();
    let mira = DiscordUserId::new(sim.participant("mira").unwrap());
    port.send_dm(
        &mira,
        OutboundMessage {
            content: "Send a seed.".to_string(),
        },
    )
    .await
    .unwrap();

    let module = mod_track("Mira seed", 3, 0);
    sim.send_message(InjectMessageRequest {
        user_id: mira.0.clone(),
        content: "This is my seed.".to_string(),
        filename: Some("mira-seed.mod".to_string()),
        bytes_base64: Some(base64::engine::general_purpose::STANDARD.encode(&module)),
        content_type: Some("application/octet-stream".to_string()),
    })
    .unwrap();

    let event = tokio::time::timeout(timeout(), session.next_event())
        .await
        .expect("an event must arrive")
        .unwrap()
        .expect("the session must stay open");
    let InboundEvent::DmAttachment {
        attachment,
        author_name,
        ..
    } = event
    else {
        panic!("expected a DM attachment, got {event:?}");
    };
    assert_eq!(attachment.filename, "mira-seed.mod");
    assert_eq!(attachment.size, 0);
    assert_eq!(author_name.as_deref(), Some("mira"));

    let fetcher = HttpAttachmentFetcher::with_base(sim.cdn_base()).unwrap();
    let fetched = fetcher.fetch(&attachment.url, 1024 * 1024).await.unwrap();
    assert_eq!(fetched, module);
}

#[tokio::test(flavor = "multi_thread")]
async fn interactions_arrive_with_their_actions() {
    let sim = DiscordSim::start().await.unwrap();
    let connector = TwilightConnector::with_proxy(
        TwilightConfig::new(sim.token(), DiscordGuildId::new(sim.guild_id())),
        sim.http_host(),
    )
    .unwrap();
    let mut session = connector.connect().await.unwrap();
    tokio::time::timeout(timeout(), session.wait_ready())
        .await
        .expect("the gateway must become ready")
        .unwrap();
    connector.register_commands().await.unwrap();
    assert!(sim.wait_commands(timeout()).await);

    let mira = sim.participant("mira").unwrap();

    let mut options = BTreeMap::new();
    options.insert(
        "timezone".to_string(),
        serde_json::json!("Europe/Stockholm"),
    );
    sim.command(mira, "join", options).unwrap();
    let event = tokio::time::timeout(timeout(), session.next_event())
        .await
        .expect("a command must arrive")
        .unwrap()
        .unwrap();
    let InboundEvent::SlashCommand {
        command, options, ..
    } = event
    else {
        panic!("expected a slash command, got {event:?}");
    };
    assert_eq!(command, swap_discord::SlashCommand::Join);
    assert_eq!(
        options.get("timezone").map(String::as_str),
        Some("Europe/Stockholm")
    );

    sim.click(mira, "swap:evt-sim:claim:tok").unwrap();
    let event = tokio::time::timeout(timeout(), session.next_event())
        .await
        .expect("a button must arrive")
        .unwrap()
        .unwrap();
    let InboundEvent::Button { custom_id, .. } = event else {
        panic!("expected a button, got {event:?}");
    };
    assert_eq!(custom_id.action(), swap_discord::Action::Claim);

    let mut fields = BTreeMap::new();
    fields.insert("title".to_string(), "A new title".to_string());
    sim.modal(mira, "swap:evt-sim:title:tok", fields).unwrap();
    let event = tokio::time::timeout(timeout(), session.next_event())
        .await
        .expect("a modal must arrive")
        .unwrap()
        .unwrap();
    let InboundEvent::ModalSubmit {
        custom_id, fields, ..
    } = event
    else {
        panic!("expected a modal, got {event:?}");
    };
    assert_eq!(custom_id.action(), swap_discord::Action::Title);
    assert_eq!(fields.get("title").map(String::as_str), Some("A new title"));
}

#[tokio::test(flavor = "multi_thread")]
async fn channels_threads_and_files_work() {
    let sim = DiscordSim::start().await.unwrap();
    let port = HttpDiscordPort::with_proxy(sim.token(), sim.http_host()).unwrap();
    let mira = DiscordUserId::new(sim.participant("mira").unwrap());
    let module = mod_track("Thread seed", 5, 0);

    let thread = port
        .create_private_thread(
            &swap_discord::DiscordChannelId::new(sim.announcements_channel()),
            "fallback for mira",
        )
        .await
        .unwrap();
    port.add_thread_member(&thread, &mira).await.unwrap();

    port.send_file_to_channel(
        &thread,
        OutboundFile {
            filename: "thread.mod".to_string(),
            caption: "Fallback file.".to_string(),
            bytes: module.clone(),
        },
    )
    .await
    .unwrap();

    let state = sim.state();
    let channel = state
        .channels
        .get(&thread.0)
        .expect("the thread must exist");
    assert_eq!(channel.kind, 12);
    assert_eq!(
        channel.parent_id.as_deref(),
        Some(sim.announcements_channel())
    );
    assert!(
        channel
            .thread_members
            .iter()
            .any(|member| member == &mira.0)
    );
    let message = state
        .messages
        .get(&thread.0)
        .and_then(|messages| messages.first())
        .expect("the thread must hold the file");
    assert_eq!(message.attachments[0].filename, "thread.mod");
}

#[tokio::test(flavor = "multi_thread")]
async fn the_setup_directory_reads_the_simulator() {
    let sim = DiscordSim::start().await.unwrap();
    let setup = TwilightSetup::with_proxy(sim.http_host());

    let identity = setup.validate_token(sim.token()).await.unwrap();
    assert_eq!(identity.id, sim.bot_user_id());
    assert_eq!(identity.username, "Monsquaz Test Bot");

    let guilds = setup.guilds(sim.token()).await.unwrap();
    assert_eq!(guilds.len(), 1);
    assert_eq!(guilds[0].id, sim.guild_id());

    let channels = setup.channels(sim.token(), sim.guild_id()).await.unwrap();
    let names = channels
        .iter()
        .map(|channel| channel.name.as_str())
        .collect::<Vec<_>>();
    assert!(names.contains(&"announcements"));
    assert!(names.contains(&"reveal"));
    assert!(names.contains(&"general"));

    let mira = sim.participant("mira").unwrap();
    setup.send_test_dm(sim.token(), mira).await.unwrap();
    assert!(
        sim.last_bot_message(mira)
            .unwrap()
            .contains("Your bot can reach you.")
    );
}
