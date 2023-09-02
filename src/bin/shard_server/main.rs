use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicI64, Ordering},
        Mutex, OnceLock,
    },
    thread,
    time::Duration,
};

use rusty_fusion::{
    net::{
        crypto::{gen_key, EncryptionMode},
        ffclient::{ClientType, FFClient},
        ffserver::FFServer,
        packet::{
            PacketID::{self, *},
            *,
        },
        LoginData,
    },
    util::get_time,
    Result,
};

const SHARD_LISTEN_ADDR: &str = "127.0.0.1:23001";
const SHARD_PUBLIC_ADDR: &str = SHARD_LISTEN_ADDR;

const LOGIN_SERVER_ADDR: &str = "127.0.0.1:23000";

const CONN_ID_DISCONNECTED: i64 = -1;
static LOGIN_SERVER_CONN_ID: AtomicI64 = AtomicI64::new(CONN_ID_DISCONNECTED);

fn main() -> Result<()> {
    let polling_interval: Duration = Duration::from_millis(50);
    let mut server: FFServer = FFServer::new(SHARD_LISTEN_ADDR, Some(polling_interval))?;

    let ls: &mut FFClient = server.connect(LOGIN_SERVER_ADDR, ClientType::LoginServer);
    login::login_connect_req(ls);
    thread::sleep(Duration::from_millis(2000));
    server.poll(&handle_packet)?;
    verify_login_server_conn();

    println!("Shard server listening on {}", server.get_endpoint());
    loop {
        server.poll(&handle_packet)?;
    }
}

fn handle_packet(
    key: &usize,
    clients: &mut HashMap<usize, FFClient>,
    pkt_id: PacketID,
) -> Result<()> {
    let client: &mut FFClient = clients.get_mut(key).unwrap();
    println!("{} sent {:?}", client.get_addr(), pkt_id);
    match pkt_id {
        P_LS2FE_REP_CONNECT_SUCC => login::login_connect_succ(client),
        P_LS2FE_REP_CONNECT_FAIL => login::login_connect_fail(client),
        P_LS2FE_REQ_UPDATE_LOGIN_INFO => login::login_update_info(client),
        //
        P_CL2LS_REQ_LOGIN => wrong_server(client),
        //
        P_CL2FE_REQ_PC_ENTER => pc_enter(client),
        P_CL2FE_REQ_PC_LOADING_COMPLETE => pc_loading_complete(client),
        P_CL2FE_GM_REQ_PC_SET_VALUE => set_player_value(client),
        P_CL2FE_REQ_PC_GOTO => player_goto(client),
        other => {
            println!("Unhandled packet: {:?}", other);
            Ok(())
        }
    }
}

fn wrong_server(client: &mut FFClient) -> Result<()> {
    let pkt: &sP_CL2LS_REQ_LOGIN = client.get_packet();
    let resp = sP_LS2CL_REP_LOGIN_FAIL {
        iErrorCode: 4, // "Login error"
        szID: pkt.szID,
    };
    client.send_packet(P_LS2CL_REP_LOGIN_FAIL, &resp)?;

    Ok(())
}

fn verify_login_server_conn() {
    let conn_id: i64 = LOGIN_SERVER_CONN_ID.load(Ordering::Relaxed);
    if conn_id == CONN_ID_DISCONNECTED {
        panic!("Couldn't handshake with login server in time");
    }
}

fn login_data() -> &'static Mutex<HashMap<i64, LoginData>> {
    static MAP: OnceLock<Mutex<HashMap<i64, LoginData>>> = OnceLock::new();
    MAP.get_or_init(|| Mutex::new(HashMap::new()))
}

fn pc_enter(client: &mut FFClient) -> Result<()> {
    let pkt: &sP_CL2FE_REQ_PC_ENTER = client.get_packet();
    let serial_key: i64 = pkt.iEnterSerialKey;
    let login_data = login_data().lock().unwrap();
    let login_data: &LoginData = login_data.get(&serial_key).unwrap();

    let resp = sP_FE2CL_REP_PC_ENTER_SUCC {
        iID: login_data.iPC_UID as i32,
        PCLoadData2CL: sPCLoadData2CL {
            iUserLevel: 1,
            PCStyle: login_data.PCStyle,
            PCStyle2: sPCStyle2 {
                iAppearanceFlag: 0,
                iTutorialFlag: 1,
                iPayzoneFlag: 0,
            },
            iLevel: 1,
            iMentor: 0,
            iMentorCount: 0,
            iHP: 9999,
            iBatteryW: 0,
            iBatteryN: 0,
            iCandy: 0,
            iFusionMatter: 0,
            iSpecialState: 0,
            iMapNum: 0,
            iX: 632032,
            iY: 187177,
            iZ: -5500,
            iAngle: 0,
            aEquip: [sItemBase {
                iType: 0,
                iID: 0,
                iOpt: 0,
                iTimeLimit: 0,
            }; 9],
            aInven: [sItemBase {
                iType: 0,
                iID: 0,
                iOpt: 0,
                iTimeLimit: 0,
            }; 50],
            aQInven: [sItemBase {
                iType: 0,
                iID: 0,
                iOpt: 0,
                iTimeLimit: 0,
            }; 50],
            aNanoBank: [sNano {
                iID: 0,
                iSkillID: 0,
                iStamina: 0,
            }; 37],
            aNanoSlots: [0; 3],
            iActiveNanoSlotNum: 0,
            iConditionBitFlag: 0,
            eCSTB___Add: 0,
            TimeBuff: sTimeBuff {
                iTimeLimit: 0,
                iTimeDuration: 0,
                iTimeRepeat: 0,
                iValue: 0,
                iConfirmNum: 0,
            },
            aQuestFlag: [0; 32],
            aRepeatQuestFlag: [0; 8],
            aRunningQuest: [sRunningQuest {
                m_aCurrTaskID: 0,
                m_aKillNPCID: [0; 3],
                m_aKillNPCCount: [0; 3],
                m_aNeededItemID: [0; 3],
                m_aNeededItemCount: [0; 3],
            }; 9],
            iCurrentMissionID: 0,
            iWarpLocationFlag: 0,
            aWyvernLocationFlag: [0; 2],
            iBuddyWarpTime: 0,
            iFatigue: 0,
            iFatigue_Level: 0,
            iFatigueRate: 0,
            iFirstUseFlag1: 0,
            iFirstUseFlag2: 0,
            aiPCSkill: [0; 33],
        },
        uiSvrTime: get_time(),
    };

    let iv1: i32 = (resp.iID + 1) as i32;
    let iv2: i32 = resp.PCLoadData2CL.iFusionMatter + 1;
    client.set_e_key(gen_key(resp.uiSvrTime, iv1, iv2));
    client.set_fe_key(login_data.uiFEKey.to_le_bytes());
    client.set_enc_mode(EncryptionMode::FEKey);

    client.send_packet(P_FE2CL_REP_PC_ENTER_SUCC, &resp)?;
    Ok(())
}

fn pc_loading_complete(client: &mut FFClient) -> Result<()> {
    let pkt: &sP_CL2FE_REQ_PC_LOADING_COMPLETE = client.get_packet();
    let resp = sP_FE2CL_REP_PC_LOADING_COMPLETE_SUCC { iPC_ID: pkt.iPC_ID };
    client.send_packet(P_FE2CL_REP_PC_LOADING_COMPLETE_SUCC, &resp)?;

    Ok(())
}

fn set_player_value(client: &mut FFClient) -> Result<()> {
    let pkt: &sP_CL2FE_GM_REQ_PC_SET_VALUE = client.get_packet();
    let resp = sP_FE2CL_GM_REP_PC_SET_VALUE {
        iPC_ID: pkt.iPC_ID,
        iSetValue: pkt.iSetValue,
        iSetValueType: pkt.iSetValueType,
    };

    client.send_packet(P_FE2CL_GM_REP_PC_SET_VALUE, &resp)?;

    Ok(())
}

fn player_goto(client: &mut FFClient) -> Result<()> {
    let pkt: &sP_CL2FE_REQ_PC_GOTO = client.get_packet();
    let resp = sP_FE2CL_REP_PC_GOTO_SUCC {
        iX: pkt.iToX,
        iY: pkt.iToY,
        iZ: pkt.iToZ,
    };

    client.send_packet(P_FE2CL_REP_PC_GOTO_SUCC, &resp)?;

    Ok(())
}

mod login {
    use std::net::SocketAddr;

    use super::*;

    pub fn login_connect_req(server: &mut FFClient) {
        let pkt = sP_FE2LS_REQ_CONNECT { iTempValue: 0 };
        server
            .send_packet(P_FE2LS_REQ_CONNECT, &pkt)
            .expect("Couldn't connect to login server");
    }

    pub fn login_connect_succ(server: &mut FFClient) -> Result<()> {
        let pkt: &sP_LS2FE_REP_CONNECT_SUCC = server.get_packet();
        let conn_id: i64 = pkt.iConn_UID;
        let conn_time: u64 = pkt.uiSvrTime;

        let iv1: i32 = (conn_id + 1) as i32;
        let iv2: i32 = 69;
        server.set_e_key(gen_key(conn_time, iv1, iv2));

        LOGIN_SERVER_CONN_ID.store(conn_id, Ordering::Relaxed);
        println!("Connected to login server ({})", server.get_addr());
        Ok(())
    }

    pub fn login_connect_fail(server: &mut FFClient) -> Result<()> {
        let pkt: &sP_LS2FE_REP_CONNECT_FAIL = server.get_packet();
        panic!("Login server refused to connect (error {})", {
            pkt.iErrorCode
        });
    }

    pub fn login_update_info(server: &mut FFClient) -> Result<()> {
        let public_addr: SocketAddr = SHARD_PUBLIC_ADDR.parse().expect("Bad public address");
        let mut ip_buf: [u8; 16] = [0; 16];
        let ip_str: &str = &public_addr.ip().to_string();
        let ip_bytes: &[u8] = ip_str.as_bytes();
        ip_buf[..ip_bytes.len()].copy_from_slice(ip_bytes);

        let pkt: &sP_LS2FE_REQ_UPDATE_LOGIN_INFO = server.get_packet();
        let resp = sP_FE2LS_REP_UPDATE_LOGIN_INFO_SUCC {
            iEnterSerialKey: pkt.iEnterSerialKey,
            g_FE_ServerIP: ip_buf,
            g_FE_ServerPort: public_addr.port() as i32,
        };

        let serial_key = resp.iEnterSerialKey;
        let mut ld = login_data().lock().unwrap();
        if ld.contains_key(&serial_key) {
            // this serial key was already registered...
            // extremely unlikely?
            let resp = sP_FE2LS_REP_UPDATE_LOGIN_INFO_FAIL {
                iEnterSerialKey: serial_key,
                iErrorCode: 1,
            };
            server.send_packet(P_FE2LS_REP_UPDATE_LOGIN_INFO_FAIL, &resp)?;
            return Ok(());
        }
        ld.insert(
            serial_key,
            LoginData {
                iPC_UID: pkt.iPC_UID,
                uiFEKey: pkt.uiFEKey,
                uiSvrTime: pkt.uiSvrTime,
                // this should ideally be fetched from DB
                PCStyle: pkt.PCStyle,
            },
        );

        server.send_packet(P_FE2LS_REP_UPDATE_LOGIN_INFO_SUCC, &resp)?;
        Ok(())
    }
}
