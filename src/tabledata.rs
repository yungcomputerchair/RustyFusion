#![allow(non_snake_case)]
#![allow(dead_code)]

use serde::{de::DeserializeOwned, Deserialize};
use serde_json::{Map, Value};
use std::{
    collections::HashMap,
    sync::OnceLock,
    time::{Duration, SystemTime},
};

use crate::{
    chunk::{EntityMap, InstanceID},
    config::config_get,
    defines::*,
    entity::NPC,
    enums::*,
    error::{log, panic_log, FFError, FFResult, Severity},
    item::{CrocPotData, Item, ItemStats, VendorData, VendorItem},
    mission::{MissionDefinition, TaskDefinition},
    nano::{NanoStats, NanoTuning},
    path::{Path, PathPoint},
    util, Position,
};

static TABLE_DATA: OnceLock<TableData> = OnceLock::new();

struct XDTData {
    vendor_data: HashMap<i32, VendorData>,
    item_data: HashMap<(i16, ItemType), ItemStats>,
    crocpot_data: HashMap<i16, CrocPotData>,
    transportation_data: TransportationData,
    instance_data: InstanceData,
    nano_data: NanoData,
    mission_data: MissionData,
}
impl XDTData {
    fn load() -> Result<Self, String> {
        let root = load_json("xdt.json")?;
        Ok(Self {
            vendor_data: load_vendor_data(&root)
                .map_err(|e| format!("Error loading vendor data: {}", e))?,
            item_data: load_item_data(&root)
                .map_err(|e| format!("Error loading item data: {}", e))?,
            crocpot_data: load_crocpot_data(&root)
                .map_err(|e| format!("Error loading crocpot data: {}", e))?,
            transportation_data: load_transportation_data(&root)
                .map_err(|e| format!("Error loading transportation data: {}", e))?,
            instance_data: load_instance_data(&root)
                .map_err(|e| format!("Error loading instance data: {}", e))?,
            nano_data: load_nano_data(&root)
                .map_err(|e| format!("Error loading nano data: {}", e))?,
            mission_data: load_mission_data(&root)
                .map_err(|e| format!("Error loading mission data: {}", e))?,
        })
    }
}

#[derive(Debug)]
struct NPCData {
    npc_type: i32,
    pos: Position,
    angle: i32,
    map_num: Option<u32>,
    followers: Vec<FollowerData>,
}

#[derive(Debug)]
struct FollowerData {
    npc_type: i32,
    offset_x: i32,
    offset_y: i32,
}

#[derive(Debug)]
pub struct TripData {
    pub npc_id: i32,
    pub transportation_type: TransportationType,
    pub start_location: i32,
    pub end_location: i32,
    pub cost: u32,
    pub speed: i32,
    pub route_number: i32,
}

#[derive(Debug)]
pub struct TransporterData {
    pub npc_type: i32,
    pub pos: Position,
}

struct TransportationData {
    trip_data: HashMap<i32, TripData>,
    scamper_data: HashMap<i32, TransporterData>,
    monkey_skyway_data: HashMap<i32, TransporterData>,
}

#[derive(Debug)]
pub struct WarpData {
    pub pos: Position,
    pub npc_type: i32,
    pub is_instance: bool,
    pub is_group_warp: bool,
    pub map_num: u32,
    pub min_level: i16,
    pub req_task: Option<(i32, i32)>, // mission id, task id
    pub req_item: Option<(ItemType, i16)>,
    pub req_item_consumed: Option<(ItemType, i16)>,
    pub cost: u32,
}

pub struct MapData {
    pub ep_id: Option<u32>,
    pub map_square: (i32, i32),
}

struct InstanceData {
    warp_data: HashMap<i32, WarpData>,
    map_data: HashMap<u32, MapData>,
}

struct NanoData {
    nano_stats: HashMap<i16, NanoStats>,
    nano_tunings: HashMap<i16, NanoTuning>,
}

struct MissionData {
    mission_definitions: HashMap<i32, MissionDefinition>,
    task_definitions: HashMap<i32, TaskDefinition>,
}

#[derive(Debug, Deserialize)]
struct CrateDropChance {
    DropChance: i32,
    DropChanceTotal: i32,
    CrateTypeDropWeights: Vec<i32>,
}

#[derive(Debug, Deserialize)]
struct CrateDropType {
    CrateIDs: Vec<i32>,
}

#[derive(Debug, Deserialize)]
struct CrateData {
    ItemSetID: i32,
    RarityWeightID: i32,
}

#[derive(Debug, Deserialize)]
struct RarityWeights {
    Weights: Vec<i32>,
}

#[derive(Debug, Deserialize)]
struct ItemSet {
    IgnoreRarity: bool,
    IgnoreGender: bool,
    DefaultItemWeight: i32,
    AlterRarityMap: HashMap<String, i32>,
    AlterGenderMap: HashMap<String, i32>,
    AlterItemWeightMap: HashMap<String, i32>,
    ItemReferenceIDs: Vec<i32>,
}

#[derive(Debug, Deserialize)]
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

struct PathData {
    skyway_paths: HashMap<i32, Path>,
    slider_path: Path,
    npc_paths: HashMap<i32, Path>,
}

pub struct TableData {
    xdt_data: XDTData,
    npc_data: Vec<NPCData>,
    drop_data: DropData,
    path_data: PathData,
}
impl TableData {
    fn new() -> Self {
        Self::load().unwrap_or_else(|e| {
            panic_log(&format!("Failed loading TableData: {}", e));
        })
    }

    fn load() -> Result<Self, String> {
        Ok(Self {
            xdt_data: XDTData::load().map_err(|e| format!("Error loading XDT: {}", e))?,
            npc_data: load_npc_data().map_err(|e| format!("Error loading NPC data: {}", e))?,
            drop_data: load_drop_data().map_err(|e| format!("Error loading drop data: {}", e))?,
            path_data: load_path_data().map_err(|e| format!("Error loading path data: {}", e))?,
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

    pub fn get_trip_data(&self, trip_id: i32) -> FFResult<&TripData> {
        self.xdt_data
            .transportation_data
            .trip_data
            .get(&trip_id)
            .ok_or(FFError::build(
                Severity::Warning,
                format!("Trip data for trip id {} doesn't exist", trip_id),
            ))
    }

    pub fn get_scamper_data(&self, location_id: i32) -> FFResult<&TransporterData> {
        self.xdt_data
            .transportation_data
            .scamper_data
            .get(&location_id)
            .ok_or(FFError::build(
                Severity::Warning,
                format!("Scamper data for location id {} doesn't exist", location_id),
            ))
    }

    pub fn get_skyway_data(&self, location_id: i32) -> FFResult<&TransporterData> {
        self.xdt_data
            .transportation_data
            .monkey_skyway_data
            .get(&location_id)
            .ok_or(FFError::build(
                Severity::Warning,
                format!(
                    "Monkey Skyway data for location id {} doesn't exist",
                    location_id
                ),
            ))
    }

    pub fn get_warp_data(&self, warp_id: i32) -> FFResult<&WarpData> {
        self.xdt_data
            .instance_data
            .warp_data
            .get(&warp_id)
            .ok_or(FFError::build(
                Severity::Warning,
                format!("Warp data for warp id {} doesn't exist", warp_id),
            ))
    }

    pub fn get_nano_stats(&self, nano_id: i16) -> FFResult<&NanoStats> {
        self.xdt_data
            .nano_data
            .nano_stats
            .get(&nano_id)
            .ok_or(FFError::build(
                Severity::Warning,
                format!("Nano stats for nano id {} doesn't exist", nano_id),
            ))
    }

    pub fn get_nano_tuning(&self, tuning_id: i16) -> FFResult<&NanoTuning> {
        self.xdt_data
            .nano_data
            .nano_tunings
            .get(&tuning_id)
            .ok_or(FFError::build(
                Severity::Warning,
                format!("Nano tuning with tuning id {} doesn't exist", tuning_id),
            ))
    }

    pub fn get_map_data(&self, map_num: u32) -> FFResult<&MapData> {
        self.xdt_data
            .instance_data
            .map_data
            .get(&map_num)
            .ok_or(FFError::build(
                Severity::Warning,
                format!("Map data for map num {} doesn't exist", map_num),
            ))
    }

    pub fn get_npcs(&self, entity_map: &mut EntityMap, channel_num: usize) -> Vec<NPC> {
        let mut npcs = Vec::new();
        for dat in &self.npc_data {
            let mut npc = NPC::new(
                entity_map.gen_next_npc_id(),
                dat.npc_type,
                Position {
                    x: dat.pos.x,
                    y: dat.pos.y,
                    z: dat.pos.z,
                },
                dat.angle,
                InstanceID {
                    channel_num,
                    map_num: dat.map_num.unwrap_or(ID_OVERWORLD),
                    instance_num: None,
                },
            );
            for follower in &dat.followers {
                let id = entity_map.gen_next_npc_id();
                let mut follower = NPC::new(
                    id,
                    follower.npc_type,
                    Position {
                        x: dat.pos.x + follower.offset_x,
                        y: dat.pos.y + follower.offset_y,
                        z: dat.pos.z,
                    },
                    dat.angle,
                    InstanceID {
                        channel_num,
                        map_num: dat.map_num.unwrap_or(ID_OVERWORLD),
                        instance_num: None,
                    },
                );
                follower.leader_id = Some(npc.id);
                npcs.push(follower);
                npc.follower_ids.insert(id);
            }
            npcs.push(npc);
        }
        npcs
    }

    pub fn get_item_from_crate(&self, crate_id: i16, gender: i32) -> FFResult<Item> {
        let crate_data =
            self.drop_data
                .crate_data
                .get(&(crate_id as i32))
                .ok_or(FFError::build(
                    Severity::Warning,
                    format!("No C.R.A.T.E. data for id {}", crate_id),
                ))?;

        let item_set =
            self.drop_data
                .item_sets
                .get(&crate_data.ItemSetID)
                .ok_or(FFError::build(
                    Severity::Warning,
                    format!("No item set with id {}", crate_data.ItemSetID),
                ))?;

        let rarity_weights = self
            .drop_data
            .rarity_weights
            .get(&crate_data.RarityWeightID)
            .ok_or(FFError::build(
                Severity::Warning,
                format!("No rarity data for id {}", crate_data.RarityWeightID),
            ))?;

        // generate a rarity from the rarity weights. rarities start at 1
        let rarity = (util::weighted_rand(&rarity_weights.Weights) + 1) as i32;

        // build a pool of eligible items
        let mut item_pool = Vec::new();
        for item_ref_id in &item_set.ItemReferenceIDs {
            let eligible: FFResult<bool> = (|| {
                let item_ref = self
                    .drop_data
                    .item_refs
                    .get(item_ref_id)
                    .ok_or(FFError::build(
                        Severity::Warning,
                        format!("No item ref with id {}", item_ref_id),
                    ))?;
                let item_stats = self
                    .get_item_stats(item_ref.ItemID as i16, (item_ref.Type as i16).try_into()?)?;
                let item_rarity = *item_set
                    .AlterRarityMap
                    .get(&item_ref_id.to_string())
                    .unwrap_or(&(item_stats.rarity.unwrap_or(0) as i32));
                let item_gender = *item_set
                    .AlterGenderMap
                    .get(&item_ref_id.to_string())
                    .unwrap_or(&(item_stats.gender.unwrap_or(0) as i32));

                // rarity checks
                if item_rarity != 0 && !item_set.IgnoreRarity && rarity != item_rarity {
                    return Ok(false);
                }

                // gender checks
                if item_gender != 0 && !item_set.IgnoreGender && gender != item_gender {
                    return Ok(false);
                }

                Ok(true)
            })();
            match eligible {
                Ok(eligible) => {
                    if eligible {
                        item_pool.push(*item_ref_id);
                    }
                }
                Err(e) => log(e.get_severity(), e.get_msg()),
            }
        }

        if item_pool.is_empty() {
            return Err(FFError::build(
                Severity::Warning,
                format!("Item pool was empty: id {}, gender {}", crate_id, gender),
            ));
        }

        // get the weights for each item
        let mut item_weights = vec![item_set.DefaultItemWeight; item_pool.len()];
        for (idx, item_ref_id) in item_pool.iter().enumerate() {
            let override_weight = item_set.AlterItemWeightMap.get(&item_ref_id.to_string());
            if let Some(weight) = override_weight {
                item_weights[idx] = *weight;
            }
        }

        // select an item
        let rolled_item_ref_id = item_pool[util::weighted_rand(&item_weights)];
        let rolled_item_ref = self.drop_data.item_refs.get(&rolled_item_ref_id).unwrap();

        Ok(Item::new(
            (rolled_item_ref.Type as i16).try_into()?,
            rolled_item_ref.ItemID as i16,
        ))
    }

    pub fn get_npc_path(&self, npc_type: i32) -> Option<Path> {
        self.path_data.npc_paths.get(&npc_type).cloned()
    }

    pub fn get_slider_path(&self) -> Path {
        self.path_data.slider_path.clone()
    }

    pub fn get_skyway_path(&self, path_id: i32) -> FFResult<Path> {
        self.path_data
            .skyway_paths
            .get(&path_id)
            .cloned()
            .ok_or(FFError::build(
                Severity::Warning,
                format!("No skyway path with id {}", path_id),
            ))
    }

    pub fn get_task_definition(&self, task_id: i32) -> FFResult<&TaskDefinition> {
        self.xdt_data
            .mission_data
            .task_definitions
            .get(&task_id)
            .ok_or(FFError::build(
                Severity::Warning,
                format!("Task with id {} doesn't exist", task_id),
            ))
    }

    pub fn get_mission_definition(&self, mission_id: i32) -> FFResult<&MissionDefinition> {
        self.xdt_data
            .mission_data
            .mission_definitions
            .get(&mission_id)
            .ok_or(FFError::build(
                Severity::Warning,
                format!("Mission with id {} doesn't exist", mission_id),
            ))
    }
}

pub fn tdata_init() -> &'static TableData {
    assert!(TABLE_DATA.get().is_none());
    let load_start = SystemTime::now();
    if TABLE_DATA.set(TableData::new()).is_err() {
        panic_log("Couldn't initialize TableData");
    }
    let load_time = load_start.elapsed().unwrap();
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

fn load_json(filename: &str) -> Result<Map<std::string::String, Value>, String> {
    let tdata_path = config_get().general.table_data_path.get();
    let path = std::path::Path::new(&tdata_path).join(filename);

    let file = std::fs::read_to_string(path.clone())
        .map_err(|e| format!("Couldn't read file {:?}: {}", path, e))?;
    let json = serde_json::from_str(&file)
        .map_err(|e| format!("Couldn't parse {:?} as JSON: {}", path, e))?;

    let Value::Object(root) = json else {
        return Err(format!("Malformed {:?}", path));
    };
    // TODO patching
    Ok(root)
}

fn get_object<'a>(
    root: &'a Map<std::string::String, Value>,
    key: &'static str,
) -> Result<&'a Map<std::string::String, Value>, String> {
    root.get(key)
        .ok_or(format!("Key missing: {}", key))
        .and_then(|v| {
            if let Value::Object(v) = v {
                Ok(v)
            } else {
                Err(format!("Value is not an object: {}", key))
            }
        })
}

fn get_array<'a>(
    root: &'a Map<std::string::String, Value>,
    key: &'static str,
) -> Result<&'a Vec<Value>, String> {
    root.get(key)
        .ok_or(format!("Key missing: {}", key))
        .and_then(|v| {
            if let Value::Array(v) = v {
                Ok(v)
            } else {
                Err(format!("Value is not an array: {}", key))
            }
        })
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
            ItemType::Head => "m_pHatItemTable",
            ItemType::Face => "m_pGlassItemTable",
            ItemType::Back => "m_pBackItemTable",
            ItemType::General => "m_pGeneralItemTable",
            ItemType::Quest => "m_pQuestItemTable",
            ItemType::Chest => "m_pChestItemTable",
            ItemType::Vehicle => "m_pVehicleItemTable",
            _ => unimplemented!(),
        };

        let item_table = get_object(root, table_key)?;
        let item_data = get_array(item_table, ITEM_TABLE_ITEM_DATA_KEY)?;
        for i in item_data {
            let data: ItemDataEntry = serde_json::from_value(i.clone())
                .map_err(|e| format!("Malformed item data entry ({:?}): {} {}", item_type, e, i))?;
            let key = (data.m_iItemNumber as i16, item_type);
            let data = ItemStats {
                buy_price: data.m_iItemPrice as u32,
                sell_price: data.m_iItemSellPrice as u32,
                sellable: data.m_iSellAble != 0,
                tradeable: data.m_iTradeAble != 0,
                max_stack_size: data.m_iStackNumber as u16,
                required_level: data.m_iMinReqLev.unwrap_or(0) as i16,
                rarity: data.m_iRarity.map(|v| v as i8),
                gender: data.m_iReqSex.map(|v| v as i8),
                speed: data.m_iUp_runSpeed,
            };
            map.insert(key, data);
        }
        Ok(())
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

    let vendor_table = get_object(root, VENDOR_TABLE_KEY)?;
    let item_data = get_array(vendor_table, VENDOR_TABLE_ITEM_DATA_KEY)?;

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

    let crocpot_table = get_object(root, CROCPOT_TABLE_KEY)?;
    let crocpot_data = get_array(crocpot_table, CROCPOT_TABLE_CROCPOT_DATA_KEY)?;
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
}

fn load_transportation_data(
    root: &Map<std::string::String, Value>,
) -> Result<TransportationData, String> {
    const TRANSPORTATION_TABLE_KEY: &str = "m_pTransportationTable";

    fn load_trip_data(
        table: &Map<std::string::String, Value>,
    ) -> Result<HashMap<i32, TripData>, String> {
        const TRIP_DATA_KEY: &str = "m_pTransportationData";

        #[derive(Debug, Deserialize)]
        struct TripDataEntry {
            m_iVehicleID: i32,
            m_iNPCID: i32,
            m_iLocalString: i32,
            m_iMoveType: i32,
            m_iStartLocation: i32,
            m_iEndLocation: i32,
            m_iCost: i32,
            m_iSpeed: i32,
            m_iMesh: i32,
            m_iSound: i32,
            m_iRouteNum: i32,
        }

        let data = get_array(table, TRIP_DATA_KEY)?;
        let mut trip_map = HashMap::new();
        for v in data {
            let trip_entry: TripDataEntry = serde_json::from_value(v.clone())
                .map_err(|e| format!("Malformed trip data entry: {} {}", e, v))?;
            let key = trip_entry.m_iVehicleID;
            if key == 0 {
                continue;
            }
            let trip_entry = TripData {
                npc_id: trip_entry.m_iNPCID,
                start_location: trip_entry.m_iStartLocation,
                end_location: trip_entry.m_iEndLocation,
                cost: trip_entry.m_iCost as u32,
                speed: trip_entry.m_iSpeed,
                route_number: trip_entry.m_iRouteNum,
                transportation_type: trip_entry
                    .m_iMoveType
                    .try_into()
                    .map_err(|e: FFError| e.get_msg().to_string())?,
            };
            trip_map.insert(key, trip_entry);
        }
        Ok(trip_map)
    }

    fn load_transporter_data(
        table: &Map<std::string::String, Value>,
        data_key: &'static str,
    ) -> Result<HashMap<i32, TransporterData>, String> {
        #[derive(Debug, Deserialize)]
        struct TransporterDataEntry {
            m_iLocationID: i32,
            m_iNPCID: i32,
            m_iXpos: i32,
            m_iYpos: i32,
            m_iZpos: i32,
            m_iZone: i32,
        }

        let data = get_array(table, data_key)?;
        let mut scamper_map = HashMap::new();
        for v in data {
            let data_entry: TransporterDataEntry = serde_json::from_value(v.clone())
                .map_err(|e| format!("Malformed transporter data entry: {} {}", e, v))?;
            let key = data_entry.m_iLocationID;
            let data_entry = TransporterData {
                npc_type: data_entry.m_iNPCID,
                pos: Position {
                    x: data_entry.m_iXpos,
                    y: data_entry.m_iYpos,
                    z: data_entry.m_iZpos,
                },
            };
            scamper_map.insert(key, data_entry);
        }
        Ok(scamper_map)
    }

    let table = get_object(root, TRANSPORTATION_TABLE_KEY)?;
    Ok(TransportationData {
        trip_data: load_trip_data(table)?,
        scamper_data: load_transporter_data(table, "m_pTransportationWarpLocation")?,
        monkey_skyway_data: load_transporter_data(table, "m_pBroomstickLocation")?,
    })
}

fn load_instance_data(root: &Map<std::string::String, Value>) -> Result<InstanceData, String> {
    const INSTANCE_TABLE_KEY: &str = "m_pInstanceTable";

    fn load_warp_data(
        table: &Map<std::string::String, Value>,
    ) -> Result<HashMap<i32, WarpData>, String> {
        const WARP_DATA_KEY: &str = "m_pWarpData";

        #[derive(Debug, Deserialize)]
        struct WarpDataEntry {
            m_iWarpNumber: i32,
            m_iWarpGroupType: i32,
            m_iNpcNumber: i32,
            m_iIsInstance: i32,
            m_iToMapNum: i32,
            m_iToX: i32,
            m_iToY: i32,
            m_iToZ: i32,
            m_iLimit_Level: i32,
            m_iLimit_TaskID: i32,
            m_iMissionID: i32,
            m_iLimit_ItemType: i32,
            m_iLimit_ItemID: i32,
            m_iLimit_UseItemType: i32,
            m_iLimit_UseItemID: i32,
            m_iCost: i32,
        }

        let data = get_array(table, WARP_DATA_KEY)?;
        let mut warp_map = HashMap::new();
        for v in data {
            let warp_data_entry: WarpDataEntry = serde_json::from_value(v.clone())
                .map_err(|e| format!("Malformed warp data entry: {} {}", e, v))?;
            let key = warp_data_entry.m_iWarpNumber;
            let data_entry = WarpData {
                pos: Position {
                    x: warp_data_entry.m_iToX,
                    y: warp_data_entry.m_iToY,
                    z: warp_data_entry.m_iToZ,
                },
                npc_type: warp_data_entry.m_iNpcNumber,
                is_instance: warp_data_entry.m_iIsInstance != 0,
                is_group_warp: warp_data_entry.m_iWarpGroupType != 0,
                map_num: warp_data_entry.m_iToMapNum as u32,
                min_level: warp_data_entry.m_iLimit_Level as i16,
                req_task: if warp_data_entry.m_iMissionID == 0 {
                    None
                } else {
                    Some((
                        warp_data_entry.m_iMissionID,
                        warp_data_entry.m_iLimit_TaskID,
                    ))
                },
                req_item: if warp_data_entry.m_iLimit_ItemID == 0 {
                    None
                } else {
                    Some((
                        (warp_data_entry.m_iLimit_ItemType as i16)
                            .try_into()
                            .map_err(|e: FFError| e.get_msg().to_string())?,
                        warp_data_entry.m_iLimit_ItemID as i16,
                    ))
                },
                req_item_consumed: if warp_data_entry.m_iLimit_UseItemID == 0 {
                    None
                } else {
                    Some((
                        (warp_data_entry.m_iLimit_UseItemType as i16)
                            .try_into()
                            .map_err(|e: FFError| e.get_msg().to_string())?,
                        warp_data_entry.m_iLimit_UseItemID as i16,
                    ))
                },
                cost: warp_data_entry.m_iCost as u32,
            };
            warp_map.insert(key, data_entry);
        }
        Ok(warp_map)
    }

    fn load_map_data(
        table: &Map<std::string::String, Value>,
    ) -> Result<HashMap<u32, MapData>, String> {
        const INSTANCE_DATA_KEY: &str = "m_pInstanceData";

        #[derive(Debug, Deserialize)]
        struct MapDataEntry {
            m_iInstanceNameID: u32,
            m_iIsEP: u32,
            m_iZoneX: i32,
            m_iZoneY: i32,
        }

        let data = get_array(table, INSTANCE_DATA_KEY)?;
        let mut map_map = HashMap::new();
        for v in data {
            let map_data_entry: MapDataEntry = serde_json::from_value(v.clone())
                .map_err(|e| format!("Malformed map data entry: {} {}", e, v))?;
            let key = map_data_entry.m_iInstanceNameID;
            let map_data_entry = MapData {
                ep_id: if map_data_entry.m_iIsEP == 0 {
                    None
                } else {
                    Some(map_data_entry.m_iIsEP as u32)
                },
                map_square: (map_data_entry.m_iZoneX, map_data_entry.m_iZoneY),
            };
            map_map.insert(key, map_data_entry);
        }
        Ok(map_map)
    }

    let table = get_object(root, INSTANCE_TABLE_KEY)?;
    Ok(InstanceData {
        warp_data: load_warp_data(table)?,
        map_data: load_map_data(table)?,
    })
}

fn load_nano_data(root: &Map<std::string::String, Value>) -> Result<NanoData, String> {
    const NANO_TABLE_KEY: &str = "m_pNanoTable";

    fn load_stats(
        table: &Map<std::string::String, Value>,
    ) -> Result<HashMap<i16, NanoStats>, String> {
        const NANO_TABLE_NANO_DATA_KEY: &str = "m_pNanoData";

        #[derive(Debug, Deserialize)]
        struct NanoStatsEntry {
            m_iNanoNumber: i32,
            m_iNanoName: i32,
            m_iComment: i32,
            m_iNanoBattery1: i32,
            m_iNanoBattery2: i32,
            m_iNanoBattery3: i32,
            m_iNanoDrain: i32,
            m_iBatteryRecharge: i32,
            m_iStyle: i32,
            m_iNanoSet: i32,
            m_iPower: i32,
            m_iAccuracy: i32,
            m_iProtection: i32,
            m_iDodge: i32,
            m_iNeedQItemID: i32,
            m_iNeedFusionMatterCnt: i32,
            m_iTune: [i16; 3],
            m_iMesh: i32,
            m_iIcon1: i32,
            m_iEffect1: i32,
            m_iSound: i32,
        }

        let nano_data = get_array(table, NANO_TABLE_NANO_DATA_KEY)?;
        let mut nano_table = HashMap::new();
        for v in nano_data {
            let nano_data_entry: NanoStatsEntry = serde_json::from_value(v.clone())
                .map_err(|e| format!("Malformed nano data entry: {} {}", e, v))?;
            let key = nano_data_entry.m_iNanoNumber as i16;
            if key == 0 {
                continue;
            }
            let nano_data_entry = NanoStats {
                style: nano_data_entry
                    .m_iStyle
                    .try_into()
                    .map_err(|e: FFError| e.get_msg().to_string())?,
                skills: nano_data_entry.m_iTune,
            };
            nano_table.insert(key, nano_data_entry);
        }
        Ok(nano_table)
    }

    pub fn load_tunings(
        table: &Map<std::string::String, Value>,
    ) -> Result<HashMap<i16, NanoTuning>, String> {
        const NANO_TABLE_NANO_TUNE_DATA_KEY: &str = "m_pNanoTuneData";

        #[derive(Debug, Deserialize)]
        struct NanoTuningEntry {
            m_iTuneNumber: i32,
            m_iReqFusionMatter: i32,
            m_iReqItemID: i32,
            m_iReqItemCount: i32,
            m_iSkillID: i32,
        }

        let nano_tuning = get_array(table, NANO_TABLE_NANO_TUNE_DATA_KEY)?;
        let mut nano_tuning_table = HashMap::new();
        for v in nano_tuning {
            let nano_tuning_entry: NanoTuningEntry = serde_json::from_value(v.clone())
                .map_err(|e| format!("Malformed nano tuning entry: {} {}", e, v))?;
            let key = nano_tuning_entry.m_iTuneNumber as i16;
            if key == 0 {
                continue;
            }
            let nano_tuning_entry = NanoTuning {
                fusion_matter_cost: nano_tuning_entry.m_iReqFusionMatter as u32,
                req_item_id: nano_tuning_entry.m_iReqItemID as i16,
                req_item_quantity: nano_tuning_entry.m_iReqItemCount as u16,
                skill_id: nano_tuning_entry.m_iSkillID as i16,
            };
            nano_tuning_table.insert(key, nano_tuning_entry);
        }
        Ok(nano_tuning_table)
    }

    let table = get_object(root, NANO_TABLE_KEY)?;
    Ok(NanoData {
        nano_stats: load_stats(table)?,
        nano_tunings: load_tunings(table)?,
    })
}

fn load_mission_data(root: &Map<std::string::String, Value>) -> Result<MissionData, String> {
    const MISSION_TABLE_KEY: &str = "m_pMissionTable";
    const MISSION_TABLE_MISSION_DATA_KEY: &str = "m_pMissionData";
    const MISSION_TABLE_MISSION_STRINGS_KEY: &str = "m_pMissionStringData";

    #[derive(Debug, Deserialize)]
    struct MissionDataEntry {
        m_iHTaskID: i32,
        m_iHMissionID: i32,
        m_iHMissionName: usize,
        m_iHNPCID: i32,
        m_iHMissionType: i32,
        m_iHTaskType: i32,
        m_iSUOutgoingTask: i32,
        m_iFOutgoingTask: i32,
        m_iCSTReqMission: [i32; MAX_REQUIRE_MISSION as usize],
        m_iCSTRReqNano: [i16; MAX_REQUIRE_NANO as usize],
        m_iCTRReqLvMin: i16,
        m_iCSTReqGuide: i16,
        m_iCSUDEFNPCID: i32,
        m_iCSTItemID: [i16; MAX_NEED_SORT_OF_ITEM as usize],
        m_iCSTItemNumNeeded: [usize; MAX_NEED_SORT_OF_ITEM as usize],
        m_iCSTTrigger: i32,
        m_iCSUCheckTimer: u64,
        m_iHTerminatorNPCID: i32,
        m_iRequireInstanceID: u32,
        m_iCSUItemID: [i16; MAX_NEED_SORT_OF_ITEM as usize],
        m_iCSUItemNumNeeded: [usize; MAX_NEED_SORT_OF_ITEM as usize],
        m_iCSUEnemyID: [i32; MAX_NEED_SORT_OF_ENEMY as usize],
        m_iCSUNumToKill: [usize; MAX_NEED_SORT_OF_ENEMY as usize],
    }

    #[derive(Debug, Deserialize)]
    struct MissionStringEntry {
        m_pstrNameString: String,
    }

    let table = get_object(root, MISSION_TABLE_KEY)?;
    let mission_data = get_array(table, MISSION_TABLE_MISSION_DATA_KEY)?;
    let mission_strings = get_array(table, MISSION_TABLE_MISSION_STRINGS_KEY)?;

    let mut mission_defs = HashMap::new();
    let mut task_defs = HashMap::new();
    for v in mission_data.iter().skip(1) {
        let entry: MissionDataEntry = serde_json::from_value(v.clone())
            .map_err(|e| format!("Malformed mission data entry: {} {}", e, v))?;
        let task_id = entry.m_iHTaskID;
        let mission_id = entry.m_iHMissionID;
        let mission_type: MissionType = entry
            .m_iHMissionType
            .try_into()
            .map_err(|e: FFError| e.get_msg().to_string())?;
        let mission_name = match mission_strings.get(entry.m_iHMissionName) {
            Some(val) => {
                let mission_string_entry: MissionStringEntry = serde_json::from_value(val.clone())
                    .map_err(|e| format!("Malformed mission string entry: {} {}", e, val))?;
                mission_string_entry.m_pstrNameString
            }
            _ => format!("Mission #{}", mission_id),
        };

        // add task to the task tree for its mission
        mission_defs
            .entry(mission_id)
            .or_insert_with(|| MissionDefinition {
                mission_id,
                mission_name,
                task_ids: Vec::new(),
                mission_type,
            })
            .task_ids
            .push(task_id);

        // create task definition
        let task_def = TaskDefinition {
            task_id,
            mission_id,
            giver_npc_type: match entry.m_iHNPCID {
                0 => None,
                x => Some(x),
            },
            task_type: entry
                .m_iHTaskType
                .try_into()
                .map_err(|e: FFError| e.get_msg().to_string())?,
            success_task_id: match entry.m_iSUOutgoingTask {
                0 => None,
                x => Some(x),
            },
            fail_task_id: match entry.m_iFOutgoingTask {
                0 => None,
                x => Some(x),
            },
            prereq_completed_mission_ids: entry
                .m_iCSTReqMission
                .iter()
                .flat_map(|id| match id {
                    0 => None,
                    x => Some(*x),
                })
                .collect(),
            prereq_nano_ids: entry
                .m_iCSTRReqNano
                .iter()
                .flat_map(|id| match id {
                    0 => None,
                    x => Some(*x),
                })
                .collect(),
            prereq_level: match entry.m_iCTRReqLvMin {
                0 => None,
                x => Some(x),
            },
            prereq_guide: match entry.m_iCSTReqGuide {
                0 => None,
                x => Some(x.try_into().map_err(|e: FFError| e.get_msg().to_string())?),
            },
            prereq_items: entry
                .m_iCSTItemID
                .iter()
                .zip(entry.m_iCSTItemNumNeeded.iter())
                .flat_map(|(&id, &num)| match id {
                    0 => None,
                    x => Some((x, num)),
                })
                .collect(),
            prereq_running_task_id: match entry.m_iCSTTrigger {
                0 => None,
                x => Some(x),
            },
            time_limit: match entry.m_iCSUCheckTimer {
                0 => None,
                x => Some(Duration::from_secs(x)),
            },
            destination_npc_type: match entry.m_iHTerminatorNPCID {
                0 => None,
                x => Some(x),
            },
            destination_map_num: match entry.m_iRequireInstanceID {
                0 => None,
                x => Some(x),
            },
            req_items: entry
                .m_iCSUItemID
                .iter()
                .zip(entry.m_iCSUItemNumNeeded.iter())
                .flat_map(|(&id, &num)| match id {
                    0 => None,
                    x => Some((x, num)),
                })
                .collect(),
            req_defeat_enemies: entry
                .m_iCSUEnemyID
                .iter()
                .zip(entry.m_iCSUNumToKill.iter())
                .flat_map(|(&id, &num)| match id {
                    0 => None,
                    x => Some((x, num)),
                })
                .collect(),
            escort_npc_type: match entry.m_iCSUDEFNPCID {
                0 => None,
                x => Some(x),
            },
        };
        task_defs.insert(task_id, task_def);
    }

    Ok(MissionData {
        mission_definitions: mission_defs,
        task_definitions: task_defs,
    })
}

fn load_npc_data() -> Result<Vec<NPCData>, String> {
    const NPC_TABLE_KEY: &str = "NPCs";
    const MOB_TABLE_KEY: &str = "mobs";
    const MOB_GROUP_TABLE_KEY: &str = "groups";

    fn load_npc_table(table: &Map<std::string::String, Value>) -> Result<Vec<NPCData>, String> {
        #[derive(Deserialize)]
        struct FollowerDataEntry {
            iNPCType: i32,
            iOffsetX: i32,
            iOffsetY: i32,
        }

        #[derive(Deserialize)]
        struct NPCDataEntry {
            aFollowers: Option<Vec<FollowerDataEntry>>,
            iAngle: i32,
            iMapNum: Option<u32>,
            iNPCType: i32,
            iX: i32,
            iY: i32,
            iZ: i32,
        }

        let mut npc_data = Vec::new();
        for (_, v) in table {
            let npc_data_entry: NPCDataEntry = serde_json::from_value(v.clone())
                .map_err(|e| format!("Malformed NPC data entry: {}", e))?;
            let npc_data_entry = NPCData {
                npc_type: npc_data_entry.iNPCType,
                pos: Position {
                    x: npc_data_entry.iX,
                    y: npc_data_entry.iY,
                    z: npc_data_entry.iZ,
                },
                angle: npc_data_entry.iAngle,
                map_num: npc_data_entry.iMapNum,
                followers: if let Some(followers) = npc_data_entry.aFollowers {
                    followers
                        .into_iter()
                        .map(|f| FollowerData {
                            npc_type: f.iNPCType,
                            offset_x: f.iOffsetX,
                            offset_y: f.iOffsetY,
                        })
                        .collect()
                } else {
                    Vec::new()
                },
            };
            npc_data.push(npc_data_entry);
        }
        Ok(npc_data)
    }

    let mut npc_data = Vec::new();

    let npc_root = load_json("NPCs.json")?;
    let npc_table = get_object(&npc_root, NPC_TABLE_KEY)?;
    npc_data.extend(load_npc_table(npc_table)?);

    let mob_root = load_json("mobs.json")?;
    let mob_table = get_object(&mob_root, MOB_TABLE_KEY)?;
    npc_data.extend(load_npc_table(mob_table)?);
    let grouped_mob_table = get_object(&mob_root, MOB_GROUP_TABLE_KEY)?;
    npc_data.extend(load_npc_table(grouped_mob_table)?);

    Ok(npc_data)
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
        table: &Map<std::string::String, Value>,
        id_key: &str,
    ) -> Result<HashMap<i32, T>, String> {
        let mut data_map = HashMap::new();
        for (_, v) in table {
            let key = v.get(id_key).ok_or(format!("Key missing: {}", id_key))?;
            let Value::Number(key) = key else {
                return Err(format!("Key not numeric: {}", key));
            };
            let key = key.as_i64().ok_or(format!("Key not an integer: {}", key))?;
            let val: T = serde_json::from_value(v.clone())
                .map_err(|e| format!("Malformed drops data entry: {}", e))?;
            data_map.insert(key as i32, val);
        }
        Ok(data_map)
    }

    let drop_root = load_json("drops.json")?;

    let crate_drop_chances_table = get_object(&drop_root, CRATE_DROP_CHANCES_TABLE_KEY)?;
    let crate_drop_types_table = get_object(&drop_root, CRATE_DROP_TYPES_TABLE_KEY)?;
    let crate_data_table = get_object(&drop_root, CRATE_DATA_TABLE_KEY)?;
    let rarity_weights_table = get_object(&drop_root, RARITY_WEIGHTS_TABLE_KEY)?;
    let item_sets_table = get_object(&drop_root, ITEM_SETS_TABLE_KEY)?;
    let item_references_table = get_object(&drop_root, ITEM_REFERENCES_TABLE_KEY)?;
    Ok(DropData {
        crate_drop_chances: load_drop_table(crate_drop_chances_table, CRATE_DROP_CHANCES_ID_KEY)?,
        crate_drop_types: load_drop_table(crate_drop_types_table, CRATE_DROP_TYPES_ID_KEY)?,
        crate_data: load_drop_table(crate_data_table, CRATE_DATA_ID_KEY)?,
        rarity_weights: load_drop_table(rarity_weights_table, RARITY_WEIGHTS_ID_KEY)?,
        item_sets: load_drop_table(item_sets_table, ITEM_SETS_ID_KEY)?,
        item_refs: load_drop_table(item_references_table, ITEM_REFERENCES_ID_KEY)?,
    })
}

fn load_path_data() -> Result<PathData, String> {
    #[derive(Deserialize)]
    struct PathPointEntry {
        iX: i32,
        iY: i32,
        iZ: i32,
        bStop: Option<bool>,
        iStopTicks: Option<usize>,
    }

    fn load_skyway_paths(
        root: &Map<std::string::String, Value>,
    ) -> Result<HashMap<i32, Path>, String> {
        const SKYWAY_TABLE_KEY: &str = "skyway";

        #[derive(Deserialize)]
        struct SkywayPathEntry {
            iRouteID: i32,
            iMonkeySpeed: i32,
            aPoints: Vec<PathPointEntry>,
        }

        let skyway_table = get_object(root, SKYWAY_TABLE_KEY)?;
        let mut skyway_paths = HashMap::new();
        for (_, v) in skyway_table {
            let skyway_path_entry: SkywayPathEntry = serde_json::from_value(v.clone())
                .map_err(|e| format!("Malformed skyway path entry: {} {}", e, v))?;
            let key = skyway_path_entry.iRouteID;
            let speed = skyway_path_entry.iMonkeySpeed;
            let mut points = Vec::new();
            for point in &skyway_path_entry.aPoints {
                points.push(PathPoint {
                    pos: Position {
                        x: point.iX,
                        y: point.iY,
                        z: point.iZ,
                    },
                    speed,
                    stop_ticks: 0,
                });
            }
            let skyway_path = Path::new(points, false);
            skyway_paths.insert(key, skyway_path);
        }
        Ok(skyway_paths)
    }

    fn load_slider_path(root: &Map<std::string::String, Value>) -> Result<Path, String> {
        const SLIDER_TABLE_KEY: &str = "slider";
        const SLIDER_SPEED: i32 = 1200;
        const SLIDER_SPEED_SLOW: i32 = 450;
        const SLIDER_STOP_TICKS: usize = 16;

        let slider_table = get_object(root, SLIDER_TABLE_KEY)?;
        let mut points = Vec::new();
        let mut was_stop = false;
        for (_, v) in slider_table {
            let point: PathPointEntry = serde_json::from_value(v.clone())
                .map_err(|e| format!("Malformed slider path entry: {} {}", e, v))?;
            let is_stop = point.bStop.unwrap();
            points.push(PathPoint {
                pos: Position {
                    x: point.iX,
                    y: point.iY,
                    z: point.iZ,
                },
                // we go slow if we're approaching or leaving a stop.
                speed: if is_stop || was_stop {
                    SLIDER_SPEED_SLOW
                } else {
                    SLIDER_SPEED
                },
                stop_ticks: if is_stop { SLIDER_STOP_TICKS } else { 0 },
            });
            was_stop = is_stop;
        }
        Ok(Path::new(points, true))
    }

    fn load_npc_paths(
        root: &Map<std::string::String, Value>,
    ) -> Result<HashMap<i32, Path>, String> {
        const NPC_TABLE_KEY: &str = "npc";

        #[derive(Deserialize)]
        struct NPCPathEntry {
            aNPCTypes: Vec<i32>,
            aNPCIDs: Vec<i64>,
            iBaseSpeed: i32,
            aPoints: Vec<PathPointEntry>,
        }

        let npc_table = get_object(root, NPC_TABLE_KEY)?;
        let mut npc_paths = HashMap::new();
        for (_, v) in npc_table {
            let npc_path_entry: NPCPathEntry = serde_json::from_value(v.clone())
                .map_err(|e| format!("Malformed NPC path entry: {} {}", e, v))?;
            let mut points = Vec::new();
            for point in &npc_path_entry.aPoints {
                points.push(PathPoint {
                    pos: Position {
                        x: point.iX,
                        y: point.iY,
                        z: point.iZ,
                    },
                    speed: npc_path_entry.iBaseSpeed,
                    stop_ticks: point.iStopTicks.unwrap(),
                });
            }
            let cycle = if points[0] == points[points.len() - 1] {
                // cyclic NPC paths in tdata have the starting point
                // duplicated at the end, but this messes up our math.
                points.pop();
                true
            } else {
                false
            };
            let npc_path = Path::new(points, cycle);
            for npc_type in &npc_path_entry.aNPCTypes {
                npc_paths.insert(*npc_type, npc_path.clone());
            }
        }
        Ok(npc_paths)
    }

    let paths_root = load_json("paths.json")?;
    Ok(PathData {
        skyway_paths: load_skyway_paths(&paths_root)?,
        slider_path: load_slider_path(&paths_root)?,
        npc_paths: load_npc_paths(&paths_root)?,
    })
}
