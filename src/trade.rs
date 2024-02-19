use crate::{
    defines::*,
    entity::Player,
    enums::*,
    error::{panic_log, FFError, FFResult, Severity},
    item::Item,
    net::packet::*,
};

#[derive(Default, Clone, Copy)]
struct TradeItem {
    pub inven_slot_num: usize,
    pub quantity: u16,
}
#[derive(Default, Clone, Copy)]
struct TradeOffer {
    taros: u32,
    items: [Option<TradeItem>; 5],
    confirmed: bool,
}
impl TradeOffer {
    fn get_count(&self, inven_slot_num: usize) -> u16 {
        let mut quantity = 0;
        for trade_item in self.items.iter().flatten() {
            if trade_item.inven_slot_num == inven_slot_num {
                quantity += trade_item.quantity;
            }
        }
        quantity
    }

    fn add_item(
        &mut self,
        trade_slot_num: usize,
        inven_slot_num: usize,
        quantity: u16,
    ) -> FFResult<u16> {
        if trade_slot_num >= self.items.len() {
            return Err(FFError::build(
                Severity::Warning,
                format!("Trade slot number {} out of range", trade_slot_num),
            ));
        }

        self.items[trade_slot_num] = Some(TradeItem {
            inven_slot_num,
            quantity,
        });

        Ok(self.get_count(inven_slot_num))
    }

    fn remove_item(&mut self, trade_slot_num: usize) -> FFResult<(u16, usize)> {
        if trade_slot_num >= self.items.len() {
            return Err(FFError::build(
                Severity::Warning,
                format!("Trade slot number {} out of range", trade_slot_num),
            ));
        }

        if self.items[trade_slot_num].is_none() {
            return Err(FFError::build(
                Severity::Warning,
                format!("Nothing in trade slot {}", trade_slot_num),
            ));
        }

        let removed_item = self.items[trade_slot_num].take().unwrap();
        Ok((
            self.get_count(removed_item.inven_slot_num),
            removed_item.inven_slot_num,
        ))
    }
}
pub struct TradeContext {
    pc_ids: [i32; 2],
    offers: [TradeOffer; 2],
}
impl TradeContext {
    pub fn new(pc_ids: [i32; 2]) -> Self {
        Self {
            pc_ids,
            offers: Default::default(),
        }
    }

    pub fn get_id_from(&self) -> i32 {
        self.pc_ids[0]
    }

    pub fn get_id_to(&self) -> i32 {
        self.pc_ids[1]
    }

    pub fn get_other_id(&self, pc_id: i32) -> i32 {
        for id in self.pc_ids {
            if id != pc_id {
                return id;
            }
        }
        panic_log("Bad trade state");
    }

    fn get_offer_mut(&mut self, pc_id: i32) -> FFResult<&mut TradeOffer> {
        let idx = self
            .pc_ids
            .iter()
            .position(|id| *id == pc_id)
            .ok_or(FFError::build(
                Severity::Warning,
                format!("Player {} is not a part of the trade", pc_id),
            ))?;
        Ok(&mut self.offers[idx])
    }

    pub fn set_taros(&mut self, pc_id: i32, taros: u32) -> FFResult<()> {
        let offer = self.get_offer_mut(pc_id)?;
        offer.taros = taros;
        offer.confirmed = false;
        Ok(())
    }

    pub fn add_item(
        &mut self,
        pc_id: i32,
        trade_slot_num: usize,
        inven_slot_num: usize,
        quantity: u16,
    ) -> FFResult<u16> {
        let offer = self.get_offer_mut(pc_id)?;
        offer.confirmed = false;
        offer.add_item(trade_slot_num, inven_slot_num, quantity)
    }

    pub fn remove_item(&mut self, pc_id: i32, trade_slot_num: usize) -> FFResult<(u16, usize)> {
        let offer = self.get_offer_mut(pc_id)?;
        offer.confirmed = false;
        offer.remove_item(trade_slot_num)
    }

    fn is_ready(&self) -> bool {
        self.offers.iter().all(|offer| offer.confirmed)
    }

    pub fn lock_in(&mut self, pc_id: i32) -> FFResult<bool> {
        let offer = self.get_offer_mut(pc_id)?;
        offer.confirmed = true;
        Ok(self.is_ready())
    }

    pub fn resolve(
        mut self,
        players: (&mut Player, &mut Player),
    ) -> FFResult<(
        [sItemTrade; SIZEOF_TRADE_SLOT as usize],
        [sItemTrade; SIZEOF_TRADE_SLOT as usize],
    )> {
        fn transfer(
            offer: &mut TradeOffer,
            from: &mut Player,
            to: &mut Player,
        ) -> FFResult<Vec<sItemTrade>> {
            // taros
            from.set_taros(from.get_taros() - offer.taros);
            to.set_taros(to.get_taros() + offer.taros);

            // items
            let mut items = Vec::new();
            for item in offer.items.iter().flatten() {
                let slot = from
                    .get_item_mut(ItemLocation::Inven, item.inven_slot_num)
                    .unwrap();
                let item_traded = Item::split_items(slot, item.quantity).unwrap();
                let free_slot = to.find_free_slot(ItemLocation::Inven)?;
                to.set_item(ItemLocation::Inven, free_slot, Some(item_traded))
                    .unwrap();
                items.push(sItemTrade {
                    iType: item_traded.ty as i16,
                    iID: item_traded.id,
                    iOpt: item_traded.quantity as i32,
                    iInvenNum: free_slot as i32,
                    iSlotNum: unused!(),
                });
            }

            Ok(items)
        }

        let blank_item = sItemTrade {
            iType: 0,
            iID: 0,
            iOpt: 0,
            iInvenNum: 0,
            iSlotNum: 0,
        };
        let mut items = (
            transfer(
                self.get_offer_mut(players.0.get_player_id()).unwrap(),
                players.0,
                players.1,
            )?,
            transfer(
                self.get_offer_mut(players.1.get_player_id()).unwrap(),
                players.1,
                players.0,
            )?,
        );
        items.0.resize(SIZEOF_TRADE_SLOT as usize, blank_item);
        items.1.resize(SIZEOF_TRADE_SLOT as usize, blank_item);
        Ok((items.1.try_into().unwrap(), items.0.try_into().unwrap()))
    }
}
