use std::str::FromStr;
use std::{collections::BTreeMap, sync::Arc};

use ethers::providers::{Http, Provider};
use tracing::info;

use crate::cache::CacheError;
use crate::models::lookup::image::Image;
use crate::models::{
    lookup::{addr::Addr, multicoin::Multicoin, text::Text, ENSLookup, LookupState},
    multicoin::cointype::coins::CoinType,
    profile::Profile,
    universal_resolver::resolve_universal,
};
use crate::utils::eip55::EIP55Address;

use super::error::ProfileError;

impl Profile {
    pub async fn from_name(
        name: &str,
        fresh: bool,
        cache: Box<dyn crate::cache::CacheLayer>,
        rpc: Provider<Http>,
        opensea_api_key: &str,
        profile_records: &Vec<String>,
        profile_chains: &Vec<CoinType>,
    ) -> Result<Self, ProfileError> {
        let cache_key = format!("n:{name}");

        info!(
            name = name,
            cache_key = cache_key,
            fresh = fresh,
            "Looking up profile for {name}..."
        );

        // If the value is in the cache, return it
        if !fresh {
            if let Ok(value) = cache.get(&cache_key).await {
                if value.is_empty() {
                    return Err(ProfileError::NotFound);
                }

                let entry_result: Result<Self, _> = serde_json::from_str(value.as_str());
                if let Ok(entry) = entry_result {
                    return Ok(entry);
                }
            }
        }

        // Preset Hardcoded Lookups
        let mut calldata: Vec<Box<dyn ENSLookup + Send + Sync>> = vec![
            Addr {}.to_boxed(),
            Image {
                // TODO: Default IPFS Gateway
                ipfs_gateway: "https://ipfs.io/ipfs/".to_string(),
                name: name.to_string(),
                record: "avatar".to_string(),
            }
            .to_boxed(),
            Image {
                // TODO: Default IPFS Gateway
                ipfs_gateway: "https://ipfs.io/ipfs/".to_string(),
                name: name.to_string(),
                record: "header".to_string(),
            }
            .to_boxed(),
            Text::new("display".to_string()).to_boxed(),
        ];

        // Lookup all Records
        let record_offset = calldata.len();
        for record in profile_records {
            calldata.push(Box::new(Text::new(record.clone())));
        }

        // Lookup all chains
        let chain_offset = calldata.len();
        for chain in profile_chains {
            calldata.push(Box::new(Multicoin {
                coin_type: chain.clone(),
            }));
        }

        let rpc = Arc::new(rpc);

        // Execute Universal Resolver Lookup
        let (data, resolver) = resolve_universal(name, &calldata, &rpc).await?;

        let mut results: Vec<Option<String>> = Vec::new();
        let mut errors = BTreeMap::default();

        let lookup_state = Arc::new(LookupState {
            rpc,
            opensea_api_key: opensea_api_key.to_string(),
        });

        // Assume results & calldata have the same length
        // Look through all calldata and decode the results at the same index
        for (index, calldata) in calldata.iter().enumerate() {
            let result = calldata.decode(&data[index], lookup_state.clone()).await;

            match result {
                Ok(result) => {
                    results.push(Some(result));
                }
                Err(error) => {
                    errors.insert(calldata.name(), error.to_string());
                    results.push(None);
                }
            }
        }

        let address = results.get(0).unwrap_or(&None).clone();
        let avatar = results.get(1).unwrap_or(&None).clone();
        let header = results.get(2).unwrap_or(&None).clone();
        let display_record = results.get(4).unwrap_or(&None).clone();

        let display = match display_record {
            Some(display) if display.to_lowercase() == name.to_lowercase() => display,
            _ => name.to_string(),
        };

        info!(
            name = name,
            address,
            avatar = ?avatar,
            header = ?header,
            "Profile for {name} found"
        );

        let mut records = BTreeMap::default();

        for (index, value) in results[record_offset..chain_offset].iter().enumerate() {
            if let Some(value) = value {
                if !value.is_empty() {
                    records.insert(profile_records[index].clone(), value.to_string());
                }
            }
        }

        let mut chains = BTreeMap::default();

        for (index, value) in results[chain_offset..].iter().enumerate() {
            if let Some(value) = value {
                if !value.is_empty() {
                    chains.insert(profile_chains[index].to_string(), value.to_string());
                }
            }
        }

        let value = Self {
            name: name.to_string(),
            address: address.and_then(|it| EIP55Address::from_str(it.as_str()).ok()),
            avatar,
            header,
            display,
            records,
            chains,
            fresh: chrono::offset::Utc::now().timestamp_millis(),
            resolver: EIP55Address(resolver),
            errors,
        };

        let response =
            serde_json::to_string(&value).map_err(|err| ProfileError::Other(err.to_string()))?;

        cache
            .set(&cache_key, &response, 600)
            .await
            .map_err(|CacheError::Other(err)| {
                ProfileError::Other(format!("cache set failed: {}", err))
            })?;

        Ok(value)
    }
}
