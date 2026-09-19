use super::{
    auth_db,
    inventory::{serialize_inventory, stack_into_bag, BagInsert},
    GameState,
};
use crate::{
    auth::AuthService,
    types::{PlayerId, ServerMessage},
};
use onlinerpg_shared::furniture_shop::{quote, FurnitureOrderLine, SHOP};

impl GameState {
    pub async fn checkout_furniture(
        &self,
        player_id: &PlayerId,
        items: Vec<FurnitureOrderLine>,
        expected_gold: i64,
        expected_total: i64,
        auth: &AuthService,
    ) {
        let error = self
            .try_checkout_furniture(player_id, &items, expected_gold, expected_total, auth)
            .await
            .err()
            .map(str::to_string);
        self.send_direct_message(player_id, ServerMessage::FurniturePurchaseResult { error })
            .await;
    }

    async fn try_checkout_furniture(
        &self,
        player_id: &PlayerId,
        items: &[FurnitureOrderLine],
        expected_gold: i64,
        expected_total: i64,
        auth: &AuthService,
    ) -> Result<(), &'static str> {
        let total = quote(items)?;
        if total != expected_total {
            return Err("Prices changed. Review your basket before paying.");
        }
        let order = items
            .iter()
            .map(|line| {
                SHOP.products
                    .iter()
                    .find(|product| product.display_ids.contains(&line.display_id))
                    .map(|product| (line, product))
                    .ok_or("That display is not for sale.")
            })
            .collect::<Result<Vec<_>, _>>()?;
        let _persistence = self.persistence_lock.lock().await;
        if self.reject_if_trading(player_id, "buy furniture").await {
            return Err("Finish your player trade first.");
        }
        let mut character = self
            .get_player_save_data(player_id)
            .await
            .ok_or("Character not found.")?;
        {
            let players = self.players.read().await;
            let player = players.get(player_id).ok_or("Character not found.")?;
            if player.health == 0
                || player.floor_level != 0
                || (player.position.x - SHOP.checkout.x).hypot(player.position.z - SHOP.checkout.z)
                    > 4.0
                || !(0.0..=4.0).contains(&player.position.y)
            {
                return Err("Visit the ORKEA exit to pay for your furniture.");
            }
        }
        let raw = self
            .terrain_io
            .read_object(SHOP.region[0], SHOP.region[1])
            .await
            .map_err(|_| "The showroom is temporarily unavailable.")?;
        let displays = Self::parse_region_furniture(&raw)
            .map_err(|_| "The showroom is temporarily unavailable.")?;
        for (line, product) in &order {
            if !displays.iter().any(|p| {
                p.id == line.display_id
                    && p.type_id == product.object_type
                    && p.x >= SHOP.bounds[0]
                    && p.x <= SHOP.bounds[2]
                    && p.z >= SHOP.bounds[1]
                    && p.z <= SHOP.bounds[3]
                    && p.floor_level == 0
            }) {
                return Err(
                    "A selected display is no longer available. Remove it from your basket.",
                );
            }
        }
        let max_weight = self.max_carry_weight(player_id).await;
        let armor_mult = self.armor_weight_mult(player_id).await;
        let mut next_id = self
            .reserve_instance_ids(items.iter().map(|i| u64::from(i.quantity)).sum())
            .await;
        let mut inventories = self.inventories.write().await;
        let inventory = inventories
            .get_mut(player_id)
            .ok_or("Inventory not found.")?;
        let mut gold = self.player_gold.write().await;
        let balance = gold.get_mut(player_id).ok_or("Gold balance not found.")?;
        if *balance != expected_gold {
            return Err("Your gold balance changed. Review your basket and try again.");
        }
        if *balance < total {
            return Err("Not enough gold.");
        }
        let mut updated = inventory.clone();
        for (line, product) in &order {
            for _ in 0..line.quantity {
                stack_into_bag(
                    &mut updated.bag,
                    BagInsert::one(false, &product.item_def_id, 0, next_id),
                );
                next_id += 1;
            }
        }
        if self.calc_total_weight(&updated, armor_mult) > max_weight {
            return Err("Your bag is too heavy. Remove some furniture or make room first.");
        }
        character.gold = *balance - total;
        let rows = serialize_inventory(&updated);
        let auth = auth.clone();
        auth_db(move || {
            auth.save_batch(
                std::slice::from_ref(&character),
                &[(character.character_id, rows)],
                &[],
                &[],
                None,
            )
        })
        .await
        .map_err(|error| {
            tracing::warn!(%error, "Furniture checkout failed");
            "Payment could not be saved. Your gold and inventory were not changed."
        })?;
        *balance -= total;
        *inventory = updated.clone();
        drop(gold);
        drop(inventories);
        self.mark_dirty(player_id).await;
        self.mark_inventory_dirty(player_id).await;
        self.send_inventory_snapshot(player_id, updated).await;
        self.send_gold_update(player_id).await;
        for (line, product) in &order {
            self.record_gold_sink(
                crate::metrics::GoldSink::ItemPurchase {
                    item_def_id: product.item_def_id.clone(),
                },
                line.quantity,
                product.price * i64::from(line.quantity),
            )
            .await;
        }
        Ok(())
    }
}
