#[derive(Debug, Clone, Copy)]
pub struct TokenRegistryEntry {
    pub contract: &'static str,
    pub symbol: &'static str,
    pub name: &'static str,
    pub issuer: &'static str,
    pub domain: &'static str,
    pub icon: &'static str,
}

const MAINNET_TOKENS: &[TokenRegistryEntry] = &[
    TokenRegistryEntry {
        contract: "CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC",
        symbol: "XLM",
        name: "XLM",
        issuer: "GDMTVHLWJTHSUDMZVVMXXH6VJHA2ZV3HNG5LYNAZ6RTWB7GISM6PGTUV",
        domain: "stellar.org",
        icon: "https://assets.coingecko.com/coins/images/100/large/Stellar_symbol_black_RGB.png",
    },
    TokenRegistryEntry {
        contract: "CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75",
        symbol: "USDC",
        name: "USDC",
        issuer: "GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN",
        domain: "centre.io",
        icon: "https://stellar.myfilebase.com/ipfs/QmNcfZxs8e9uVyhEa3xoPWCsj3ZogGirtixMEC9Km4Fjm2",
    },
    TokenRegistryEntry {
        contract: "CDTKPWPLOURQA2SGTKTUQOWRCBZEORB4BWBOMJ3D3ZTQQSGE5F6JBQLV",
        symbol: "EURC",
        name: "EURC",
        issuer: "GDHU6WRG4IEQXM5NZ4BMPKOXHW76MZM4Y2IEMFDVXBSDP6SJY4ITNPP2",
        domain: "circle.com",
        icon: "https://stellar.myfilebase.com/ipfs/QmeRk7LG85cozSNey9QGARgbxYi1cG1dA1G6SNJGMTMdF2",
    },
    TokenRegistryEntry {
        contract: "CCCRWH6Q3FNP3I2I57BDLM5AFAT7O6OF6GKQOC6SSJNDAVRZ57SPHGU2",
        symbol: "PYUSD",
        name: "PYUSD",
        issuer: "GDQE7IXJ4HUHV6RQHIUPRJSEZE4DRS5WY577O2FY6YQ5LVWZ7JZTU2V5",
        domain: "paxos.com",
        icon: "https://assets.coingecko.com/coins/images/31212/standard/PYUSD_Token_Logo_2x.png",
    },
    TokenRegistryEntry {
        contract: "CB3YA656OYIHU57657I5KGSBRHE5I3OZU4VFC22PYAOANFZHEWNYGAGP",
        symbol: "USDY",
        name: "USDY",
        issuer: "GAJMPX5NBOG6TQFPQGRABJEEB2YE7RFRLUKJDZAZGAD5GFX4J7TADAZ6",
        domain: "ondo.finance",
        icon: "https://assets.coingecko.com/coins/images/31700/standard/usdy_(1).png",
    },
    TokenRegistryEntry {
        contract: "CBLV4ATSIWU67CFSQU2NVRKINQIKUZ2ODSZBUJTJ43VJVRSBTZYOPNUR",
        symbol: "USTRY",
        name: "USTRY",
        issuer: "GCRYUGD5NVARGXT56XEZI5CIFCQETYHAPQQTHO2O3IQZTHDH4LATMYWC",
        domain: "etherfuse.com",
        icon: "https://assets.coingecko.com/coins/images/52361/standard/-STABLEBOND-06.jpg",
    },
    TokenRegistryEntry {
        contract: "CAL6ER2TI6CTRAY6BFXWNWA7WTYXUXTQCHUBCIBU5O6KM3HJFG6Z6VXV",
        symbol: "CETES",
        name: "CETES",
        issuer: "GCRYUGD5NVARGXT56XEZI5CIFCQETYHAPQQTHO2O3IQZTHDH4LATMYWC",
        domain: "etherfuse.com",
        icon: "https://assets.coingecko.com/coins/images/37855/standard/cetes.png",
    },
    TokenRegistryEntry {
        contract: "CBIJBDNZNF4X35BJ4FFZWCDBSCKOP5NB4PLG4SNENRMLAPYG4P5FM6VN",
        symbol: "SolvBTC",
        name: "Solv BTC",
        issuer: "",
        domain: "solv.finance",
        icon: "https://raw.githubusercontent.com/solv-finance/solv-resources/main/SolvBTC/SolvBTC.svg",
    },
    TokenRegistryEntry {
        contract: "CAUP7NFABXE5TJRL3FKTPMWRLC7IAXYDCTHQRFSCLR5TMGKHOOQO772J",
        symbol: "xSolvBTC",
        name: "xSolvBTC",
        issuer: "",
        domain: "solv.finance",
        icon: "https://raw.githubusercontent.com/solv-finance/solv-resources/main/xSolvBTC/xSolvBTC.svg",
    },
];

pub fn find_token(contract: &str) -> Option<TokenRegistryEntry> {
    MAINNET_TOKENS
        .iter()
        .copied()
        .find(|entry| entry.contract.eq_ignore_ascii_case(contract))
}
