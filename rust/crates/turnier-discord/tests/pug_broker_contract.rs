use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

use dl_broker::port::*;
use dl_broker::{router, BrokerState};
use serde_json::json;
use sqlx::postgres::PgPoolOptions;
use turnier_config::Config;
use turnier_discord::{BrokerClient, BrokerError, DiscordNotifier};

const GUILD: u64 = 1_300_000_000_000_000_001;
const ROLE: u64 = 1_300_000_000_000_000_002;
const MEMBER: u64 = 1_300_000_000_000_000_003;
const CHANNEL: u64 = 1_300_000_000_000_000_004;
const MISSING_MEMBER: u64 = 1_300_000_000_000_000_005;

#[derive(Default)]
struct TestDiscord {
    live_queries: Mutex<Vec<(u64, u64)>>,
    moves: Mutex<Vec<(u64, u64, Option<u64>)>>,
}

#[async_trait::async_trait]
impl DiscordPort for TestDiscord {
    async fn live_role_members(
        &self,
        guild_id: u64,
        role_id: u64,
    ) -> Result<RoleMembers, PortError> {
        self.live_queries.lock().unwrap().push((guild_id, role_id));
        if role_id != ROLE {
            return Err(PortError::RoleNotFound);
        }
        Ok(RoleMembers {
            role_id,
            name: "Caster".into(),
            members: vec![MemberInfo {
                user_id: MEMBER,
                display_name: "Test Caster".into(),
            }],
        })
    }

    async fn move_voice(
        &self,
        guild_id: u64,
        user_id: u64,
        channel_id: Option<u64>,
    ) -> Result<(), PortError> {
        self.moves
            .lock()
            .unwrap()
            .push((guild_id, user_id, channel_id));
        if user_id == MEMBER {
            Ok(())
        } else {
            Err(PortError::MemberNotFound)
        }
    }

    async fn is_ready(&self) -> bool {
        unreachable!("unexpected is_ready")
    }

    async fn send_channel_message(
        &self,
        _channel_id: u64,
        _content: &str,
    ) -> Result<u64, PortError> {
        unreachable!("unexpected send_channel_message")
    }

    async fn send_channel_reply(
        &self,
        _channel_id: u64,
        _reply: &ChannelReply,
        _content: &str,
    ) -> Result<u64, PortError> {
        unreachable!("unexpected send_channel_reply")
    }

    async fn send_dm(
        &self,
        _user_id: u64,
        _content: &str,
        _purpose: DmPurpose,
    ) -> Result<(Option<u64>, u64), PortError> {
        unreachable!("unexpected send_dm")
    }

    async fn create_text_channel(
        &self,
        _category_id: u64,
        _name: &str,
        _topic: Option<&str>,
    ) -> Result<u64, PortError> {
        unreachable!("unexpected create_text_channel")
    }

    async fn delete_channel(&self, _channel_id: u64) -> Result<(), PortError> {
        unreachable!("unexpected delete_channel")
    }

    async fn delete_message(
        &self,
        _channel_id: u64,
        _message_id: u64,
        _reason: &str,
    ) -> Result<(), PortError> {
        unreachable!("unexpected delete_message")
    }

    async fn fetch_message_reactions(
        &self,
        _channel_id: u64,
        _message_id: u64,
    ) -> Result<Vec<MessageReaction>, PortError> {
        unreachable!("unexpected fetch_message_reactions")
    }

    async fn add_reaction(
        &self,
        _channel_id: u64,
        _message_id: u64,
        _emoji: &str,
    ) -> Result<(), PortError> {
        unreachable!("unexpected add_reaction")
    }

    async fn send_rich_message(&self, _message: &RichMessage) -> Result<u64, PortError> {
        unreachable!("unexpected send_rich_message")
    }

    async fn edit_rich_message(
        &self,
        _message_id: u64,
        _message: &RichMessage,
    ) -> Result<(), PortError> {
        unreachable!("unexpected edit_rich_message")
    }

    async fn add_role(
        &self,
        _guild_id: u64,
        _user_id: u64,
        _role_id: u64,
        _reason: &str,
    ) -> Result<(), PortError> {
        unreachable!("unexpected add_role")
    }

    async fn create_role(
        &self,
        _guild_id: u64,
        _name: &str,
        _mentionable: bool,
        _reason: &str,
    ) -> Result<u64, PortError> {
        unreachable!("unexpected create_role")
    }

    async fn remove_role(
        &self,
        _guild_id: u64,
        _user_id: u64,
        _role_id: u64,
        _reason: &str,
    ) -> Result<(), PortError> {
        unreachable!("unexpected remove_role")
    }

    async fn voice_members(&self, _channel_id: u64) -> Result<Vec<MemberInfo>, PortError> {
        unreachable!("unexpected voice_members")
    }

    async fn create_invite(
        &self,
        _channel_id: u64,
        _reason: &str,
    ) -> Result<InviteInfo, PortError> {
        unreachable!("unexpected create_invite")
    }

    async fn list_roles(&self, _guild_id: Option<u64>) -> Result<GuildRoles, PortError> {
        unreachable!("unexpected list_roles")
    }

    async fn role_members(
        &self,
        _guild_id: Option<u64>,
        _role_id: u64,
    ) -> Result<RoleMembers, PortError> {
        unreachable!("unexpected role_members")
    }

    async fn member_access(
        &self,
        _guild_id: Option<u64>,
        _user_id: u64,
    ) -> Result<MemberAccess, PortError> {
        unreachable!("unexpected member_access")
    }

    async fn member_present(
        &self,
        _guild_id: u64,
        _user_id: u64,
    ) -> Result<MemberPresence, PortError> {
        unreachable!("unexpected member_present")
    }

    async fn resolve_names(&self, _user_ids: &[u64]) -> Result<Vec<MemberInfo>, PortError> {
        unreachable!("unexpected resolve_names")
    }

    async fn resolve_user(&self, _user_id: u64) -> Result<Option<ResolvedUser>, PortError> {
        unreachable!("unexpected resolve_user")
    }

    async fn list_members(&self) -> Result<Vec<GuildMemberInfo>, PortError> {
        unreachable!("unexpected list_members")
    }

    async fn guild_stats(&self, _guild_id: Option<u64>) -> Result<GuildStats, PortError> {
        unreachable!("unexpected guild_stats")
    }
}

#[tokio::test]
async fn pug_caster_and_voice_match_the_real_broker_router() {
    let port = Arc::new(TestDiscord::default());
    let token = "test-only-broker-token";
    let state = BrokerState::new(port.clone(), token.into(), |_| None).unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        axum::serve(
            listener,
            router(state).into_make_service_with_connect_info::<SocketAddr>(),
        )
        .await
        .unwrap();
    });

    let pool = PgPoolOptions::new()
        .connect_lazy("postgres://test@127.0.0.1:9/unused")
        .unwrap();
    let client = BrokerClient::new(&format!("http://{addr}"), token);
    let notifier = DiscordNotifier::new(client, pool, &Config::default());

    let members = notifier
        .get_role_members(GUILD as i64, ROLE as i64)
        .await
        .unwrap();
    assert_eq!(
        members,
        vec![json!({"id": MEMBER.to_string(), "display_name": "Test Caster"})]
    );

    let missing_role = notifier
        .get_role_members(GUILD as i64, (ROLE + 1) as i64)
        .await;
    assert!(matches!(
        missing_role,
        Err(BrokerError::Http { status: 503, .. })
    ));
    assert_eq!(
        *port.live_queries.lock().unwrap(),
        vec![(GUILD, ROLE), (GUILD, ROLE + 1)]
    );

    let result = notifier
        .move_users_to_voice_channel(
            &[
                MEMBER.to_string(),
                "-1".into(),
                MEMBER.to_string(),
                MISSING_MEMBER.to_string(),
            ],
            CHANNEL as i64,
            GUILD as i64,
        )
        .await;
    assert_eq!(result.moved, vec![MEMBER.to_string()]);
    assert_eq!(result.skipped_guests, vec!["-1"]);
    assert_eq!(result.failed.len(), 1);
    assert_eq!(result.failed[0].discord_id, MISSING_MEMBER.to_string());
    assert!(!result.failed[0].error.is_empty());
    assert_eq!(
        *port.moves.lock().unwrap(),
        vec![
            (GUILD, MEMBER, Some(CHANNEL)),
            (GUILD, MISSING_MEMBER, Some(CHANNEL))
        ]
    );
    server.abort();
}
