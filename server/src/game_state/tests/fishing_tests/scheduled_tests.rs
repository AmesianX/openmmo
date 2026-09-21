use super::*;
use crate::game_state::fishing::FishingPhase;
use onlinerpg_shared::schedule::ScheduleEntry;

async fn scheduled_angler(game: &GameState) -> (PlayerId, DirectRx, ScheduleEntry) {
    let (id, rx) = make_angler(game, "Tobin").await;
    let entry = ScheduleEntry {
        at: "00:00".into(),
        pos: [-100.0, 0.0, 50.0],
        rotation: -89.0,
        action: Some("fishing".into()),
        ..Default::default()
    };
    {
        let mut players = game.players.write().await;
        let player = players.get_mut(&id).unwrap();
        player.is_official_npc = true;
        player.rotation = entry.rotation.to_radians();
    }
    game.set_npc_schedule("Tobin", vec![entry.clone()]);
    (id, rx, entry)
}

#[tokio::test(start_paused = true)]
async fn scheduled_fishing_keeps_its_heading_and_line_without_rewards() {
    let game = make_test_game_state("scheduled_fishing_idle");
    let (id, mut rx, entry) = scheduled_angler(&game).await;
    game.start_fishing(&id, entry.fishing_target().unwrap())
        .await;
    let messages = drain(&mut rx);
    assert!(messages.iter().any(|message| matches!(message,
        ServerMessage::FishingCasted { rotation, .. }
            if *rotation == entry.rotation.to_radians()
    )));

    for action in [
        FishingAction::Hook,
        FishingAction::Reel,
        FishingAction::GiveLine,
    ] {
        game.respond_fishing(&id, action).await;
    }
    advance(Duration::from_secs(24 * 60 * 60)).await;
    game.tick_fishing(None).await;
    assert!(matches!(
        game.fishing_sessions.read().await[&id].phase,
        FishingPhase::Scheduled
    ));
    assert!(drain(&mut rx).is_empty());
    assert!(game.inventories.read().await[&id].bag.is_empty());

    game.set_npc_schedule("Tobin", vec![]);
    game.tick_fishing(None).await;
    assert!(!game.fishing_sessions.read().await.contains_key(&id));
    assert!(drain(&mut rx).iter().any(|message| matches!(
        message,
        ServerMessage::FishingEnded {
            outcome: FishingOutcome::Aborted,
            ..
        }
    )));
}

#[tokio::test(start_paused = true)]
async fn scheduled_fishing_requires_an_official_npc_at_the_scheduled_spot() {
    for official_at_spot in [false, true] {
        let game = make_test_game_state("scheduled_fishing_guards");
        let (id, mut rx, mut entry) = scheduled_angler(&game).await;
        if official_at_spot {
            entry.pos[0] -= 10.0;
            game.set_npc_schedule("Tobin", vec![entry]);
        } else {
            game.players
                .write()
                .await
                .get_mut(&id)
                .unwrap()
                .is_official_npc = false;
        }
        game.start_fishing(&id, water_target()).await;
        advance_until_bite(&game, &mut rx).await;
        assert!(matches!(
            game.fishing_sessions.read().await[&id].phase,
            FishingPhase::Bite { .. }
        ));
    }
}

#[tokio::test(start_paused = true)]
async fn scheduled_fishing_stops_when_the_rod_is_put_away() {
    let game = make_test_game_state("scheduled_fishing_unequip");
    let (id, mut rx, entry) = scheduled_angler(&game).await;
    game.start_fishing(&id, entry.fishing_target().unwrap())
        .await;
    drain(&mut rx);
    game.unequip_item(&id, EquipSlot::MainHand).await;
    assert!(!game.fishing_sessions.read().await.contains_key(&id));
    assert!(drain(&mut rx).iter().any(|message| matches!(
        message,
        ServerMessage::FishingEnded {
            outcome: FishingOutcome::Aborted,
            ..
        }
    )));
}
