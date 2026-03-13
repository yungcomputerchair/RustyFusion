use std::{net::IpAddr, sync::OnceLock};

use maxminddb::{geoip2::City, Reader};

use crate::error::{FFError, FFResult, Severity};

static GEO_DB_READER: OnceLock<Reader<Vec<u8>>> = OnceLock::new();

#[derive(Debug, Clone)]
pub struct GeoInfo {
    pub coords: (f64, f64),
    pub city_name: Option<String>,
}

fn load_geo_db(path: &str) -> Result<(), String> {
    let reader =
        Reader::open_readfile(path).map_err(|e| format!("Failed to open GeoIP database: {}", e))?;
    GEO_DB_READER
        .set(reader)
        .map_err(|_| "GeoIP database already initialized".to_string())
}

pub fn geo_init(geo_db_path: &str) -> FFResult<()> {
    assert!(GEO_DB_READER.get().is_none());
    if let Err(e) = load_geo_db(geo_db_path) {
        return Err(FFError::build(
            Severity::Warning,
            format!(
                "GeoIP initialization failed: {}. Geo-based shard routing disabled.",
                e
            ),
        ));
    }
    Ok(())
}

pub fn haversine_distance(pos1: (f64, f64), pos2: (f64, f64)) -> f64 {
    const EARTH_RADIUS_KM: f64 = 6371.0;

    let (lat1, lon1) = pos1;
    let (lat2, lon2) = pos2;

    let to_radians = |deg: f64| deg * std::f64::consts::PI / 180.0;
    let lat1_rad = to_radians(lat1);
    let lat2_rad = to_radians(lat2);
    let delta_lat = to_radians(lat2 - lat1);
    let delta_lon = to_radians(lon2 - lon1);

    let a = (delta_lat / 2.0).sin().powi(2)
        + lat1_rad.cos() * lat2_rad.cos() * (delta_lon / 2.0).sin().powi(2);
    let c = 2.0 * a.sqrt().atan2((1.0 - a).sqrt());

    EARTH_RADIUS_KM * c
}

pub fn do_lookup(ip: IpAddr) -> Option<GeoInfo> {
    let reader = GEO_DB_READER.get()?;

    if ip.is_loopback() {
        return None;
    }

    let lookup = reader.lookup(ip).ok()?;
    let city = lookup.decode::<City>().ok()??;
    let latitude = city.location.latitude?;
    let longitude = city.location.longitude?;
    let city_name = city.city.names.english.map(|cn| cn.to_string());

    Some(GeoInfo {
        coords: (latitude, longitude),
        city_name,
    })
}
