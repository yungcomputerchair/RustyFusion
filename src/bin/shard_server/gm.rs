use super::*;

pub fn gm_pc_set_value(client: &mut FFClient) -> Result<()> {
    let pkt: &sP_CL2FE_GM_REQ_PC_SET_VALUE = client.get_packet();
    let resp = sP_FE2CL_GM_REP_PC_SET_VALUE {
        iPC_ID: pkt.iPC_ID,
        iSetValue: pkt.iSetValue,
        iSetValueType: pkt.iSetValueType,
    };

    client.send_packet(P_FE2CL_GM_REP_PC_SET_VALUE, &resp)?;

    Ok(())
}
