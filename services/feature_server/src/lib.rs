use std::collections::HashMap;

use parking_lot::RwLock;

#[derive(Debug, PartialEq)]
pub enum FeatureStoreReadError {
    KeyDoesNotExists,
}

#[derive(Default)]
pub struct FeatureStore {
    store: RwLock<HashMap<String, RwLock<Vec<f32>>>>,
}

impl FeatureStore {
    pub fn get_feature(&self, feature_id: &str) -> Result<Vec<f32>, FeatureStoreReadError> {
        let data = self.store.read();
        let feats_id = data
            .get(feature_id)
            .ok_or(FeatureStoreReadError::KeyDoesNotExists)?;
        let feats = feats_id.read().clone();
        Ok(feats)
    }
    pub fn batch_update_features(&self, updates: impl IntoIterator<Item = (String, Vec<f32>)>) {
        for (id, feats) in updates.into_iter() {
            let data = self.store.read();
            let feats_id = data.get(id.as_str());
            if let Some(feats_id) = feats_id {
                let mut curr_feats = feats_id.write();
                *curr_feats = feats;
            } else {
                drop(data);
                let mut data = self.store.write();
                data.insert(id, RwLock::new(feats));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_store_read() {
        let data: RwLock<HashMap<String, RwLock<Vec<f32>>>> = RwLock::new(HashMap::from([(
            "feat_1".to_string(),
            RwLock::new(vec![0.1, 0.2]),
        )]));
        let feature_store = FeatureStore { store: data };
        let feats = feature_store.get_feature("feat_1");
        match feats {
            Ok(feats) => assert_eq!(vec![0.1, 0.2], feats, "Wrong data retrieval"),
            Err(_) => panic!("Key should be found"),
        }
        let feats = feature_store.get_feature("feat_2");
        match feats {
            Ok(_) => panic!("Feature should not be found"),
            Err(err) => assert_eq!(
                FeatureStoreReadError::KeyDoesNotExists,
                err,
                "Wrong error type"
            ),
        }
    }
    #[test]
    fn test_store_write() {
        let data: HashMap<String, Vec<f32>> =
            HashMap::from([("feat_1".to_string(), vec![0.1, 0.2])]);
        let store = FeatureStore {
            store: RwLock::new(HashMap::new()),
        };
        store.batch_update_features(data);
        let feats = store.get_feature("feat_1");
        match feats {
            Ok(feats) => assert_eq!(vec![0.1, 0.2], feats, "Wrong data inserted"),
            Err(_) => panic!("Key not inserted"),
        }
    }
}
