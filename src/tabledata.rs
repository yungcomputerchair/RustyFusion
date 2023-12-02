#![allow(non_snake_case)]

use serde::Deserialize;
use serde_json::{Map, Value};
use std::{collections::HashMap, sync::OnceLock};

use crate::{
    defines::SIZEOF_VENDOR_TABLE_SLOT,
    error::{log, FFError, FFResult, Severity},
    net::packet::{sItemBase, sItemVendor},
    npc::NPC,
};

static TABLE_DATA: OnceLock<TableData> = OnceLock::new();

#[derive(Clone)]
struct VendorData {
    sort_number: i32,
    ty: i16,
    id: i16,
    price: i32,
}

struct XDTData {
    vendor_data: HashMap<i32, Vec<VendorData>>,
}
impl XDTData {
    fn load() -> Result<Self, String> {
        let raw = load_json("tabledata/xdt.json")?;
        if let Value::Object(root) = raw {
            Ok(Self {
                vendor_data: load_vendor_data(&root)?,
            })
        } else {
            Err(format!("Bad XDT tabledata (root): {}", raw))
        }
    }
}

#[derive(Deserialize)]
struct NPCData {
    iNPCType: i32,
    iX: i32,
    iY: i32,
    iZ: i32,
    iAngle: i32,
    iMapNum: Option<i32>,
}

pub struct TableData {
    xdt_data: XDTData,
    npc_data: HashMap<i32, NPCData>,
}
impl TableData {
    fn new() -> Self {
        Self::load().unwrap_or_else(|e| {
            log(Severity::Fatal, &e);
            log(Severity::Fatal, "Failed loading TableData");
            panic!();
        })
    }

    fn load() -> Result<Self, String> {
        Ok(Self {
            xdt_data: XDTData::load()?,
            npc_data: load_npc_data()?,
        })
    }

    pub fn get_vendor_data(
        &self,
        vendor_id: i32,
    ) -> FFResult<[sItemVendor; SIZEOF_VENDOR_TABLE_SLOT as usize]> {
        let vendor_items = self
            .xdt_data
            .vendor_data
            .get(&vendor_id)
            .ok_or(FFError::build(
                Severity::Warning,
                format!("Vendor with ID {} doesn't exist", vendor_id),
            ))?;
        let mut vendor_item_structs = Vec::new();
        for item in vendor_items {
            vendor_item_structs.push(sItemVendor {
                iVendorID: vendor_id,
                fBuyCost: item.price as f32,
                item: sItemBase {
                    iType: item.ty,
                    iID: item.id,
                    iOpt: 1,
                    iTimeLimit: 0,
                },
                iSortNum: item.sort_number,
            });
        }
        vendor_item_structs.resize(
            SIZEOF_VENDOR_TABLE_SLOT as usize,
            sItemVendor {
                iVendorID: 0,
                fBuyCost: 0.0,
                item: sItemBase::default(),
                iSortNum: 0,
            },
        );
        Ok(vendor_item_structs.try_into().unwrap())
    }

    pub fn get_npcs(&self) -> impl Iterator<Item = NPC> + '_ {
        self.npc_data.iter().map(|(npc_id, npc_data)| -> NPC {
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
}

pub fn tdata_init() -> &'static TableData {
    assert!(TABLE_DATA.get().is_none());
    if TABLE_DATA.set(TableData::new()).is_err() {
        panic!("Couldn't initialize TableData");
    }
    log(Severity::Info, "Loaded TableData");
    tdata_get()
}

pub fn tdata_get() -> &'static TableData {
    assert!(TABLE_DATA.get().is_some());
    TABLE_DATA.get().unwrap()
}

fn load_json(path: &str) -> Result<Value, String> {
    let file =
        std::fs::read_to_string(path).map_err(|e| format!("Couldn't read file {}: {}", path, e))?;
    serde_json::from_str(&file).map_err(|e| format!("Couldn't parse {} as JSON: {}", path, e))

    // TODO patching
}

fn load_vendor_data(
    root: &Map<std::string::String, Value>,
) -> Result<HashMap<i32, Vec<VendorData>>, String> {
    const VENDOR_TABLE_KEY: &str = "m_pVendorTable";
    const VENDOR_TABLE_ITEM_DATA_KEY: &str = "m_pItemData";

    #[derive(Deserialize)]
    struct VendorDataRaw {
        m_iNpcNumber: i32,
        m_iSortNumber: i32,
        m_iItemType: i16,
        m_iitemID: i16,
        m_iSellCost: i32,
    }

    let vendor_table = root
        .get(VENDOR_TABLE_KEY)
        .ok_or(format!("Key missing: {}", VENDOR_TABLE_KEY))?;
    if let Value::Object(vendor_table) = vendor_table {
        let item_data = vendor_table
            .get(VENDOR_TABLE_ITEM_DATA_KEY)
            .ok_or(format!("Key missing: {}", VENDOR_TABLE_ITEM_DATA_KEY))?;
        if let Value::Array(item_data) = item_data {
            let mut vendor_data = HashMap::new();
            for v in item_data {
                let vendor_data_entry: VendorDataRaw = serde_json::from_value(v.clone())
                    .map_err(|e| format!("Malformed vendor data entry: {}", e))?;
                let key = vendor_data_entry.m_iNpcNumber;
                let vendor_data_entry = VendorData {
                    sort_number: vendor_data_entry.m_iSortNumber,
                    ty: vendor_data_entry.m_iItemType,
                    id: vendor_data_entry.m_iitemID,
                    price: vendor_data_entry.m_iSellCost,
                };

                vendor_data
                    .entry(key)
                    .or_insert_with(Vec::new)
                    .push(vendor_data_entry);
            }
            Ok(vendor_data)
        } else {
            Err(format!(
                "Array missing: {}.{}",
                VENDOR_TABLE_KEY, VENDOR_TABLE_ITEM_DATA_KEY
            ))
        }
    } else {
        Err(format!("Object missing: {}", VENDOR_TABLE_KEY,))
    }
}

fn load_npc_data() -> Result<HashMap<i32, NPCData>, String> {
    const NPC_TABLE_KEY: &str = "NPCs";

    let raw = load_json("tabledata/NPCs.json")?;
    if let Value::Object(root) = raw {
        let npcs = root
            .get("NPCs")
            .ok_or(format!("Key missing: {}", NPC_TABLE_KEY))?;
        if let Value::Object(npcs) = npcs {
            let mut npc_data = HashMap::new();
            for (k, v) in npcs {
                let npc_id: i32 = k.parse().map_err(|_| format!("Bad NPC data ID: {}", k))?;
                let npc_data_entry: NPCData = serde_json::from_value(v.clone())
                    .map_err(|e| format!("Malformed NPC data entry: {}", e))?;
                npc_data.insert(npc_id, npc_data_entry);
            }
            Ok(npc_data)
        } else {
            panic!("Bad NPC tabledata (root.NPCs): {npcs}");
        }
    } else {
        Err(format!("Malformed NPC tabledata root: {}", raw))
    }
}
