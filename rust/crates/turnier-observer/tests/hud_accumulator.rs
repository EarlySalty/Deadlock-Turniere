use turnier_observer::hud::{BossKill, DecodedTick, HudAccumulator, HudEvent, HudPhase};

fn fixture() -> Vec<DecodedTick> {
    serde_json::from_str(include_str!("fixtures/hud_ticks.json")).expect("fixture")
}

fn accumulate(ticks: &[DecodedTick]) -> HudAccumulator {
    let mut acc = HudAccumulator::default();
    for tick in ticks {
        acc.apply_tick(tick);
    }
    acc
}

#[test]
fn snapshot_tracks_clock_souls_and_scoreline() {
    let ticks = fixture();
    let snapshot = accumulate(&ticks).snapshot();
    let last = &ticks[1];
    let rules = last.rules.as_ref().expect("rules");
    let expected_clock = last.tick as f64 / 64.0
        - rules.game_start_time.expect("start")
        - rules.total_paused_ticks.expect("paused") as f64 / 64.0;

    assert_eq!(snapshot.phase, HudPhase::Live);
    assert_eq!(snapshot.match_id, Some(131416010));
    assert_eq!(snapshot.clock_seconds, Some(expected_clock));
    assert_eq!(snapshot.bans, vec![81, 64, 86]);
    assert_eq!(snapshot.players.len(), 12);
    assert_eq!(snapshot.players.iter().filter(|p| p.team == 0).count(), 6);

    for team in 0..2u8 {
        let rows = last
            .players
            .iter()
            .filter(|p| p.team_num == Some(i64::from(team) + 2));
        let souls: i64 = rows.clone().filter_map(|p| p.net_worth).sum();
        let kills: i64 = rows.filter_map(|p| p.kills).sum();
        assert_eq!(snapshot.teams[team as usize].souls, Some(souls));
        assert_eq!(snapshot.teams[team as usize].kills, Some(kills));
    }
    let point = snapshot.soul_difference.last().expect("soul point");
    assert_eq!(
        point.diff,
        snapshot.teams[0].souls.unwrap_or_default() - snapshot.teams[1].souls.unwrap_or_default()
    );
    let mut ranks: Vec<u32> = snapshot
        .players
        .iter()
        .filter_map(|p| p.net_worth_rank)
        .collect();
    ranks.sort_unstable();
    assert_eq!(ranks, (1..=12).collect::<Vec<_>>());
}

#[test]
fn new_upgrade_becomes_item_event_but_baseline_does_not() {
    let ticks = fixture();
    let first = accumulate(&ticks[..1]).snapshot();
    assert!(first.item_events.is_empty());

    let snapshot = accumulate(&ticks).snapshot();
    assert_eq!(snapshot.item_events.len(), 1);
    assert!(matches!(
        snapshot.item_events[0],
        HudEvent::Item {
            slot: 11,
            team: 0,
            item_id: 3190916303,
            ..
        }
    ));
}

#[test]
fn early_respawn_is_a_revive_and_dead_player_shows_timer() {
    let mut ticks = fixture();
    let respawn_at = ticks[1].tick as f64 / 64.0 + 20.0;
    let dead = &mut ticks[0].players[0];
    dead.alive = Some(false);
    dead.respawn_time = Some(respawn_at);
    let slot = dead.slot;

    let dead_snapshot = accumulate(&ticks[..1]).snapshot();
    let dead_player = dead_snapshot
        .players
        .iter()
        .find(|p| p.slot == slot)
        .expect("player");
    assert_eq!(dead_player.alive, Some(false));
    let expected = (respawn_at - ticks[0].tick as f64 / 64.0).ceil();
    assert_eq!(dead_player.respawn_seconds, Some(expected));

    let snapshot = accumulate(&ticks).snapshot();
    assert!(snapshot
        .feed_events
        .iter()
        .any(|event| matches!(event, HudEvent::Revive { slot: s, .. } if *s == slot)));
}

#[test]
fn unknown_values_stay_unknown_and_winner_ends_match() {
    let mut ticks = fixture();
    ticks[1].players[0].net_worth = None;
    ticks[1].players[0].health_max = None;
    let mut acc = accumulate(&ticks);
    let snapshot = acc.snapshot();
    let player = snapshot
        .players
        .iter()
        .find(|p| p.slot == ticks[1].players[0].slot)
        .expect("player");
    assert_eq!(player.net_worth, None);
    assert_eq!(player.net_worth_rank, None);
    assert_eq!(player.health_fraction, None);

    let kill = BossKill {
        tick: ticks[1].tick,
        team_num: Some(3),
        objective: "walker".into(),
    };
    acc.apply_boss_kill(&kill);
    acc.apply_boss_kill(&kill);
    let mut ended = ticks[1].clone();
    ended.tick += 64;
    if let Some(rules) = ended.rules.as_mut() {
        rules.winning_team = Some(2);
    }
    acc.apply_tick(&ended);
    let snapshot = acc.snapshot();
    assert_eq!(snapshot.phase, HudPhase::Ended);
    assert_eq!(snapshot.winner, Some(0));
    assert!(matches!(
        snapshot.feed_events.last(),
        Some(HudEvent::Objective { team: Some(1), .. })
    ));
    let objectives = snapshot
        .feed_events
        .iter()
        .filter(|event| matches!(event, HudEvent::Objective { .. }))
        .count();
    assert_eq!(objectives, 1);
}
