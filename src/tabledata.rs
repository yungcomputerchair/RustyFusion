#![allow(non_snake_case)]

use serde::Deserialize;
use serde_json::Value::{self, *};
use std::{collections::HashMap, fs::File, io::BufReader, sync::OnceLock};

use crate::{
    error::{log, Severity},
    npc::NPC,
};

static TABLE_DATA: OnceLock<TableData> = OnceLock::new();

#[derive(Deserialize)]
struct NPCData {
    iNPCType: i32,
    iX: i32,
    iY: i32,
    iZ: i32,
    iAngle: i32,
    iMapNum: Option<i32>,
}

struct TableData {
    npc_data: HashMap<i32, NPCData>,
}
impl TableData {
    fn new() -> Self {
        Self {
            npc_data: load_npc_data(),
        }
    }
}

pub fn tdata_init() {
    assert!(TABLE_DATA.get().is_none());
    if TABLE_DATA.set(TableData::new()).is_err() {
        panic!("Couldn't load TableData");
    }
    log(Severity::Info, "Loaded TableData");
}

fn load_npc_data() -> HashMap<i32, NPCData> {
    let raw: Value =
        serde_json::from_reader(BufReader::new(File::open("tabledata/NPCs.json").unwrap()))
            .unwrap();

    // TODO patching

    let mut npc_data = HashMap::new();
    if let Object(root) = raw {
        let npcs = root.get("NPCs").expect("Key missing: 'NPCs'");
        if let Object(npcs) = npcs {
            for (k, v) in npcs {
                let npc_id: i32 = k
                    .parse()
                    .unwrap_or_else(|err| panic!("Bad NPC tabledata ID (root.NPCs.{k}): {err}"));
                let npc_data_entry: NPCData = serde_json::from_value(v.clone())
                    .unwrap_or_else(|err| panic!("Bad NPC tabledata entry (root.NPCs.{k}): {err}"));
                npc_data.insert(npc_id, npc_data_entry);
            }
        } else {
            panic!("Bad NPC tabledata (root.NPCs): {npcs}");
        }
    } else {
        panic!("Bad NPC tabledata (root): {raw}");
    }
    npc_data
}

pub fn tdata_get_npcs() -> impl Iterator<Item = NPC> {
    let tdata = TABLE_DATA.get().expect("TableData not initialized");
    tdata.npc_data.iter().map(|(npc_id, npc_data)| -> NPC {
        NPC::new(
            *npc_id,
            npc_data.iNPCType,
            npc_data.iX,
            npc_data.iY,
            npc_data.iZ,
            npc_data.iAngle,
            npc_data.iMapNum.unwrap_or(0) as u64,
        )
    })
}
