#![allow(non_snake_case)]
#![allow(dead_code)]

use serde::{de::DeserializeOwned, Deserialize};
use serde_json::{Map, Value};
use std::{collections::HashMap, sync::OnceLock, time::SystemTime};

use crate::{
    enums::ItemType,
    error::{log, FFError, FFResult, Severity},
    npc::NPC,
    CrocPotData, ItemStats, VendorData, VendorItem,
};

static TABLE_DATA: OnceLock<TableData> = OnceLock::new();

struct XDTData {
    vendor_data: HashMap<i32, VendorData>,
    item_data: HashMap<(i16, ItemType), ItemStats>,
    crocpot_data: HashMap<i16, CrocPotData>,
}
impl XDTData {
    fn load() -> Result<Self, String> {
        let raw = load_json("tabledata/xdt.json")?;
        if let Value::Object(root) = raw {
            Ok(Self {
                vendor_data: load_vendor_data(&root)?,
                item_data: load_item_data(&root)?,
                crocpot_data: load_crocpot_data(&root)?,
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

#[derive(Deserialize)]
struct CrateDropChance {
    DropChance: i32,
    DropChanceTotal: i32,
    CrateTypeDropWeights: Vec<i32>,
}

#[derive(Deserialize)]
struct CrateDropType {
    CrateIDs: Vec<i32>,
}

#[derive(Deserialize)]
struct CrateData {
    ItemSetID: i32,
    RarityWeightID: i32,
}

#[derive(Deserialize)]
struct RarityWeights {
    Weights: Vec<i32>,
}

#[derive(Deserialize)]
struct ItemSet {
    IgnoreRarity: bool,
    IgnoreGender: bool,
    DefaultItemWeight: i32,
    AlterRarityMap: HashMap<String, i32>,
    AlterGenderMap: HashMap<String, i32>,
    AlterItemWeightMap: HashMap<String, i32>,
    ItemReferenceIDs: Vec<i32>,
}

#[derive(Deserialize)]
struct ItemReference {
    ItemID: i32,
    Type: i32,
}

struct DropData {
    crate_drop_chances: HashMap<i32, CrateDropChance>,
    crate_drop_types: HashMap<i32, CrateDropType>,
    crate_data: HashMap<i32, CrateData>,
    rarity_weights: HashMap<i32, RarityWeights>,
    item_sets: HashMap<i32, ItemSet>,
    item_refs: HashMap<i32, ItemReference>,
}

pub struct TableData {
    xdt_data: XDTData,
    npc_data: Vec<NPCData>,
    drop_data: DropData,
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
            drop_data: load_drop_data()?,
        })
    }

    pub fn get_item_stats(&self, item_id: i16, item_type: ItemType) -> FFResult<&ItemStats> {
        self.xdt_data
            .item_data
            .get(&(item_id, item_type))
            .ok_or(FFError::build(
                Severity::Warning,
                format!(
                    "Item with ID {} and type {:?} doesn't exist",
                    item_id, item_type
                ),
            ))
    }

    pub fn get_vendor_data(&self, vendor_id: i32) -> FFResult<&VendorData> {
        self.xdt_data
            .vendor_data
            .get(&vendor_id)
            .ok_or(FFError::build(
                Severity::Warning,
                format!("Vendor with ID {} doesn't exist", vendor_id),
            ))
    }

    pub fn get_crocpot_data(&self, level_gap: i16) -> FFResult<&CrocPotData> {
        self.xdt_data
            .crocpot_data
            .get(&level_gap)
            .ok_or(FFError::build(
                Severity::Warning,
                format!("CrocPotOdds for level gap {} don't exist", level_gap),
            ))
    }

    pub fn get_npcs(&self) -> impl Iterator<Item = NPC> + '_ {
        self.npc_data
            .iter()
            .enumerate()
            .map(|(npc_id, npc_data)| -> NPC {
                NPC::new(
                    npc_id as i32,
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
    let load_start = SystemTime::now();
    if TABLE_DATA.set(TableData::new()).is_err() {
        panic!("Couldn't initialize TableData");
    }
    let load_time = SystemTime::now().duration_since(load_start).unwrap();
    log(
        Severity::Info,
        &format!("Loaded TableData ({:.2}s)", load_time.as_secs_f32()),
    );
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

fn load_item_data(
    root: &Map<std::string::String, Value>,
) -> Result<HashMap<(i16, ItemType), ItemStats>, String> {
    const ITEM_TABLE_ITEM_DATA_KEY: &str = "m_pItemData";

    fn load_item_data_for_type(
        root: &Map<std::string::String, Value>,
        map: &mut HashMap<(i16, ItemType), ItemStats>,
        item_type: ItemType,
    ) -> Result<(), String> {
        #[derive(Deserialize)]
        struct ItemDataEntry {
            m_iItemNumber: i32,
            m_iItemName: Option<i32>,
            m_iComment: Option<i32>,
            m_iTradeAble: i32,
            m_iItemPrice: i32,
            m_iItemSellPrice: i32,
            m_iSellAble: i32,
            m_iStackNumber: i16,
            m_iIcon: Option<i32>,
            m_fStyleMod_TrumpMonster: Option<f32>,
            m_fStyleMod_Trumped: Option<f32>,
            m_iEquipLoc: Option<i32>,
            m_iEquipType: Option<i32>,
            m_ibattery: Option<i32>,
            m_iBatteryDrain: Option<i32>,
            m_iMinReqLev: Option<i32>,
            m_iMentor: Option<i32>,
            m_iAtkRange: Option<i32>,
            m_iAtkAngle: Option<i32>,
            m_iAtkRate: Option<i32>,
            m_iEffectArea: Option<i32>,
            m_iTargetMode: Option<i32>,
            m_iTargetNumber: Option<i32>,
            m_iInitalTime: Option<i32>,
            m_iDeliverTime: Option<i32>,
            m_iDelayTime: Option<i32>,
            m_iDurationTime: Option<i32>,
            m_iUp_power: Option<i32>,
            m_iUp_accuracy: Option<i32>,
            m_iUp_protection: Option<i32>,
            m_iUp_dodge: Option<i32>,
            m_iUp_runSpeed: Option<i32>,
            m_iUp_swimSpeed: Option<i32>,
            m_iUp_jumpHeight: Option<i32>,
            m_iUp_jumpDistance: Option<i32>,
            m_iUp_atkRate: Option<i32>,
            m_iUp_effectArea: Option<i32>,
            m_iUp_addFusionMatter: Option<i32>,
            m_iUp_addCandy: Option<i32>,
            m_iUp_addItemfind: Option<i32>,
            m_iMesh: Option<i32>,
            m_iTexture: Option<i32>,
            m_iTexture2: Option<i32>,
            m_iEffect1: Option<i32>,
            m_iSound1: Option<i32>,
            m_iReqSex: Option<i32>,
            m_iRarity: Option<i32>,
            m_iPointRat: Option<i32>,
            m_iGroupRat: Option<i32>,
            m_iDefenseRat: Option<i32>,
            m_iEffect2: Option<i32>,
            m_iSound2: Option<i32>,
            m_iCashAble: Option<i32>,
        }

        let table_key = match item_type {
            ItemType::Hand => "m_pWeaponItemTable",
            ItemType::UpperBody => "m_pShirtsItemTable",
            ItemType::LowerBody => "m_pPantsItemTable",
            ItemType::Foot => "m_pShoesItemTable",
            ItemType::Head => "m_pHeadItemTable",
            ItemType::Face => "m_pGlassItemTable",
            ItemType::Back => "m_pBackItemTable",
            ItemType::General => "m_pGeneralItemTable",
            ItemType::Quest => "m_pQuestItemTable",
            ItemType::Chest => "m_pChestItemTable",
            ItemType::Vehicle => "m_pVehicleItemTable",
            _ => unimplemented!(),
        };

        let item_table = root
            .get(table_key)
            .ok_or(format!("Key missing: {}", table_key))?;
        if let Value::Object(item_table) = item_table {
            let item_data = item_table.get(ITEM_TABLE_ITEM_DATA_KEY).ok_or(format!(
                "Key missing: {}.{}",
                table_key, ITEM_TABLE_ITEM_DATA_KEY
            ))?;
            if let Value::Array(item_data) = item_data {
                for i in item_data {
                    let data: ItemDataEntry = serde_json::from_value(i.clone()).map_err(|e| {
                        format!("Malformed item data entry ({:?}): {} {}", item_type, e, i)
                    })?;
                    let key = (data.m_iItemNumber as i16, item_type);
                    let data = ItemStats {
                        buy_price: data.m_iItemPrice as u32,
                        sell_price: data.m_iItemSellPrice as u32,
                        sellable: data.m_iSellAble != 0,
                        tradeable: data.m_iTradeAble != 0,
                        max_stack_size: data.m_iStackNumber as u16,
                        required_level: data.m_iMinReqLev.unwrap_or(0) as i16,
                        rarity: data.m_iRarity.map(|v| v as i8),
                    };
                    map.insert(key, data);
                }
                Ok(())
            } else {
                Err(format!(
                    "Array missing: {}.{}",
                    table_key, ITEM_TABLE_ITEM_DATA_KEY
                ))
            }
        } else {
            Err(format!("Object missing: {}", table_key))
        }
    }

    let mut map = HashMap::new();
    load_item_data_for_type(root, &mut map, ItemType::Hand)?;
    load_item_data_for_type(root, &mut map, ItemType::UpperBody)?;
    load_item_data_for_type(root, &mut map, ItemType::LowerBody)?;
    load_item_data_for_type(root, &mut map, ItemType::Foot)?;
    load_item_data_for_type(root, &mut map, ItemType::Head)?;
    load_item_data_for_type(root, &mut map, ItemType::Face)?;
    load_item_data_for_type(root, &mut map, ItemType::Back)?;
    load_item_data_for_type(root, &mut map, ItemType::Vehicle)?;
    load_item_data_for_type(root, &mut map, ItemType::General)?;
    load_item_data_for_type(root, &mut map, ItemType::Chest)?;
    Ok(map)
}

fn load_vendor_data(
    root: &Map<std::string::String, Value>,
) -> Result<HashMap<i32, VendorData>, String> {
    const VENDOR_TABLE_KEY: &str = "m_pVendorTable";
    const VENDOR_TABLE_ITEM_DATA_KEY: &str = "m_pItemData";

    #[derive(Deserialize)]
    struct VendorDataEntry {
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
        let item_data = vendor_table.get(VENDOR_TABLE_ITEM_DATA_KEY).ok_or(format!(
            "Key missing: {}.{}",
            VENDOR_TABLE_KEY, VENDOR_TABLE_ITEM_DATA_KEY
        ))?;
        if let Value::Array(item_data) = item_data {
            let mut vendor_data = HashMap::new();
            for v in item_data {
                let vendor_data_entry: VendorDataEntry = serde_json::from_value(v.clone())
                    .map_err(|e| format!("Malformed vendor data entry: {} {}", e, v))?;
                let key = vendor_data_entry.m_iNpcNumber;
                let vendor_data_entry = VendorItem {
                    sort_number: vendor_data_entry.m_iSortNumber,
                    ty: vendor_data_entry
                        .m_iItemType
                        .try_into()
                        .map_err(|e: FFError| e.get_msg().to_string())?,
                    id: vendor_data_entry.m_iitemID,
                };

                vendor_data
                    .entry(key)
                    .or_insert_with(|| VendorData::new(key))
                    .insert(vendor_data_entry);
            }
            Ok(vendor_data)
        } else {
            Err(format!(
                "Array missing: {}.{}",
                VENDOR_TABLE_KEY, VENDOR_TABLE_ITEM_DATA_KEY
            ))
        }
    } else {
        Err(format!("Object missing: {}", VENDOR_TABLE_KEY))
    }
}

fn load_crocpot_data(
    root: &Map<std::string::String, Value>,
) -> Result<HashMap<i16, CrocPotData>, String> {
    const CROCPOT_TABLE_KEY: &str = "m_pCombiningTable";
    const CROCPOT_TABLE_CROCPOT_DATA_KEY: &str = "m_pCombiningData";

    #[derive(Deserialize)]
    struct CrocPotDataEntry {
        m_iLevelGap: i32,
        m_fSameGrade: f32,
        m_fOneGrade: f32,
        m_fTwoGrade: f32,
        m_fThreeGrade: f32,
        m_fLevelGapStandard: f32,
        m_iLookConstant: i32,
        m_iStatConstant: i32,
    }

    let crocpot_table = root
        .get(CROCPOT_TABLE_KEY)
        .ok_or(format!("Key missing: {}", CROCPOT_TABLE_KEY))?;
    if let Value::Object(crocpot_table) = crocpot_table {
        let crocpot_data = crocpot_table
            .get(CROCPOT_TABLE_CROCPOT_DATA_KEY)
            .ok_or(format!(
                "Key missing: {}.{}",
                CROCPOT_TABLE_KEY, CROCPOT_TABLE_CROCPOT_DATA_KEY
            ))?;
        if let Value::Array(crocpot_data) = crocpot_data {
            let mut crocpot_table = HashMap::new();
            for v in crocpot_data {
                let crocpot_data_entry: CrocPotDataEntry = serde_json::from_value(v.clone())
                    .map_err(|e| format!("Malformed crocpot data entry: {} {}", e, v))?;
                let key = crocpot_data_entry.m_iLevelGap as i16;
                let crocpot_odds = CrocPotData {
                    base_chance: crocpot_data_entry.m_fLevelGapStandard / 100.0,
                    rarity_diff_multipliers: [
                        crocpot_data_entry.m_fSameGrade / 100.0,
                        crocpot_data_entry.m_fOneGrade / 100.0,
                        crocpot_data_entry.m_fTwoGrade / 100.0,
                        crocpot_data_entry.m_fThreeGrade / 100.0,
                    ],
                    price_multiplier_looks: crocpot_data_entry.m_iLookConstant as u32,
                    price_multiplier_stats: crocpot_data_entry.m_iStatConstant as u32,
                };
                crocpot_table.insert(key, crocpot_odds);
            }
            Ok(crocpot_table)
        } else {
            Err(format!(
                "Array missing: {}.{}",
                CROCPOT_TABLE_KEY, CROCPOT_TABLE_CROCPOT_DATA_KEY
            ))
        }
    } else {
        Err(format!("Object missing: {}", CROCPOT_TABLE_KEY))
    }
}

fn load_npc_data() -> Result<Vec<NPCData>, String> {
    const NPC_TABLE_KEY: &str = "NPCs";

    let raw = load_json("tabledata/NPCs.json")?;
    if let Value::Object(root) = raw {
        let npcs = root
            .get(NPC_TABLE_KEY)
            .ok_or(format!("Key missing: {}", NPC_TABLE_KEY))?;
        if let Value::Object(npcs) = npcs {
            let mut npc_data = Vec::new();
            for (_, v) in npcs {
                let npc_data_entry: NPCData = serde_json::from_value(v.clone())
                    .map_err(|e| format!("Malformed NPC data entry: {}", e))?;
                npc_data.push(npc_data_entry);
            }
            Ok(npc_data)
        } else {
            panic!("Bad NPC tabledata (root.NPCs): {npcs}");
        }
    } else {
        Err(format!("Malformed NPC tabledata root: {}", raw))
    }
}

fn load_drop_data() -> Result<DropData, String> {
    const CRATE_DROP_CHANCES_TABLE_KEY: &str = "CrateDropChances";
    const CRATE_DROP_TYPES_TABLE_KEY: &str = "CrateDropTypes";
    const CRATE_DATA_TABLE_KEY: &str = "Crates";
    const RARITY_WEIGHTS_TABLE_KEY: &str = "RarityWeights";
    const ITEM_SETS_TABLE_KEY: &str = "ItemSets";
    const ITEM_REFERENCES_TABLE_KEY: &str = "ItemReferences";

    const CRATE_DROP_CHANCES_ID_KEY: &str = "CrateDropChanceID";
    const CRATE_DROP_TYPES_ID_KEY: &str = "CrateDropTypeID";
    const CRATE_DATA_ID_KEY: &str = "CrateID";
    const RARITY_WEIGHTS_ID_KEY: &str = "RarityWeightID";
    const ITEM_SETS_ID_KEY: &str = "ItemSetID";
    const ITEM_REFERENCES_ID_KEY: &str = "ItemReferenceID";

    fn load_drop_table<T: DeserializeOwned>(
        root: &Value,
        table_key: &str,
        id_key: &str,
    ) -> Result<HashMap<i32, T>, String> {
        if let Value::Object(root) = root {
            let data = root
                .get(table_key)
                .ok_or(format!("Key missing: {}", table_key))?;
            if let Value::Object(data) = data {
                let mut data_map = HashMap::new();
                for (n, v) in data {
                    let key = v.get(id_key).ok_or(format!("Key missing: {}", id_key))?;
                    if let Value::Number(key) = key {
                        let key = key.as_i64().ok_or(format!("Key not i64: {}", key))?;
                        let val: T = serde_json::from_value(v.clone())
                            .map_err(|e| format!("Malformed drops data entry: {}", e))?;
                        data_map.insert(key as i32, val);
                    } else {
                        return Err(format!(
                            "Bad drops tabledata key (root.{}.{}): {}",
                            table_key, n, key
                        ));
                    }
                }
                Ok(data_map)
            } else {
                Err(format!(
                    "Bad drops tabledata (root.{}): {}",
                    table_key, data
                ))
            }
        } else {
            Err(format!("Malformed drops tabledata root: {}", root))
        }
    }

    let root = load_json("tabledata/drops.json")?;
    Ok(DropData {
        crate_drop_chances: load_drop_table(
            &root,
            CRATE_DROP_CHANCES_TABLE_KEY,
            CRATE_DROP_CHANCES_ID_KEY,
        )?,
        crate_drop_types: load_drop_table(
            &root,
            CRATE_DROP_TYPES_TABLE_KEY,
            CRATE_DROP_TYPES_ID_KEY,
        )?,
        crate_data: load_drop_table(&root, CRATE_DATA_TABLE_KEY, CRATE_DATA_ID_KEY)?,
        rarity_weights: load_drop_table(&root, RARITY_WEIGHTS_TABLE_KEY, RARITY_WEIGHTS_ID_KEY)?,
        item_sets: load_drop_table(&root, ITEM_SETS_TABLE_KEY, ITEM_SETS_ID_KEY)?,
        item_refs: load_drop_table(&root, ITEM_REFERENCES_TABLE_KEY, ITEM_REFERENCES_ID_KEY)?,
    })
}
