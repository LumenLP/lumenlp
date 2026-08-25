//! Map Soroban contract IDs + token meta → Freighter token ids.

pub const NATIVE_SAC_MAINNET: &str = "CAS3J7GYLGXMF6TDJBBYYSE3HQ6BBSMLNUQ34T6TZMYMW2EVH34XOWMA";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FreighterAssetId {
    Native,
    Classic { code: String, issuer: String },
}

impl FreighterAssetId {
    pub fn as_freighter_key(&self) -> String {
        match self {
            Self::Native => "native".to_string(),
            Self::Classic { code, issuer } => format!("{code}:{issuer}"),
        }
    }
}

/// Resolve Freighter id from contract address, optional registry issuer,
/// optional on-chain name.
pub fn resolve_freighter_asset_id(
    contract: &str,
    symbol: Option<&str>,
    name: Option<&str>,
    issuer: Option<&str>,
) -> Option<FreighterAssetId> {
    if contract.eq_ignore_ascii_case(NATIVE_SAC_MAINNET)
        || symbol.is_some_and(|s| s.eq_ignore_ascii_case("native"))
        || name.is_some_and(|n| n.eq_ignore_ascii_case("native"))
    {
        return Some(FreighterAssetId::Native);
    }

    if let Some(iss) = issuer.map(str::trim).filter(|s| !s.is_empty()) {
        if let Some(sym) = symbol
            .map(str::trim)
            .filter(|s| !s.is_empty() && !s.eq_ignore_ascii_case("native"))
        {
            return Some(FreighterAssetId::Classic {
                code: sym.to_string(),
                issuer: iss.to_string(),
            });
        }
    }

    name.and_then(parse_code_issuer)
        .map(|(code, issuer)| FreighterAssetId::Classic { code, issuer })
}

fn parse_code_issuer(raw: &str) -> Option<(String, String)> {
    let (code, issuer) = raw.split_once(':')?;
    if code.is_empty() || issuer.len() < 56 {
        return None;
    }
    if !issuer.chars().all(|c| c.is_ascii_alphanumeric()) {
        return None;
    }
    Some((code.to_string(), issuer.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_sac_maps_to_native() {
        let id = resolve_freighter_asset_id(NATIVE_SAC_MAINNET, Some("native"), Some("native"), None).expect("native");
        assert_eq!(id, FreighterAssetId::Native);
        assert_eq!(id.as_freighter_key(), "native");
    }

    #[test]
    fn classic_from_issuer_and_symbol() {
        let id = resolve_freighter_asset_id(
            "CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75",
            Some("USDC"),
            Some("USDC"),
            Some("GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN"),
        )
        .expect("usdc");
        assert_eq!(
            id.as_freighter_key(),
            "USDC:GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN"
        );
    }

    #[test]
    fn classic_from_name_code_issuer() {
        let id = resolve_freighter_asset_id(
            "CAUIKL3IYGMERDRUN6YSCLWVAKIFG5Q4YJHUKM4S4NJZQIA3BAS6OJPK",
            Some("AQUA"),
            Some("AQUA:GBNZILSTVQZ4R7IKQDGHYGY2QXL5QOFJYQMXPKWRRM5PAV7Y4M67AQUA"),
            None,
        )
        .expect("aqua");
        assert_eq!(
            id.as_freighter_key(),
            "AQUA:GBNZILSTVQZ4R7IKQDGHYGY2QXL5QOFJYQMXPKWRRM5PAV7Y4M67AQUA"
        );
    }

    #[test]
    fn unmapped_sep41_without_issuer_is_none() {
        assert!(resolve_freighter_asset_id(
            "CBIJBDNZNF4X35BJ4FFZWCDBSCKOP5NB4PLG4SNENRMLAPYG4P5FM6VN",
            Some("SolvBTC"),
            Some("Solv BTC"),
            None,
        )
        .is_none());
    }
}
