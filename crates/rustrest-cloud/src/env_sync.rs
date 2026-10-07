//! environment sync. an environment syncs as one document (`name` + `data`, guarded by `rev`),
//! merged variable by variable against the base (the server's copy as of the last sync): a change only
//! one side made is kept, and when both sides changed the same variable, ours wins.

use std::collections::{BTreeSet, HashMap, HashSet};

use rustrest_core::KeyValuePair;
use rustrest_core::collection::env::{EnvCloudLink, Environment};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::api::{CloudClient, CloudError};
use crate::wire::CloudEnvironment;

/// a push that keeps hitting newer server copies gives up after this many tries
const MAX_PUSH_ROUNDS: usize = 3;

/// one variable as stored in the cloud
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SharedVar {
    pub key: String,
    /// `None` for local variables
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    #[serde(default = "enabled_default")]
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub local: bool,
}

fn enabled_default() -> bool {
    true
}

/// the environment's `data` document on the server
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SharedData {
    #[serde(default)]
    pub variables: Vec<SharedVar>,
}

/// everything about an environment that syncs
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Shared {
    pub name: String,
    pub data: SharedData,
}

impl Shared {
    /// what of `env` the team gets to see: local values are left out, and
    /// rows without a key or repeating an earlier key are skipped
    pub fn of(env: &Environment) -> Self {
        let mut seen = HashSet::new();
        let variables = env
            .variables
            .iter()
            .filter_map(|var| {
                let key = var.key.trim();
                if key.is_empty() || !seen.insert(key) {
                    return None;
                }
                let local = env.is_local(key);
                Some(SharedVar {
                    key: key.to_string(),
                    value: (!local).then(|| var.value.clone()),
                    enabled: var.is_active,
                    local,
                })
            })
            .collect();
        Self {
            name: env.name.clone(),
            data: SharedData { variables },
        }
    }

    /// a server copy. data in a shape we don't understand reads as empty
    pub fn from_server(name: &str, data: &Value) -> Self {
        let data: SharedData = serde_json::from_value(data.clone()).unwrap_or_default();
        // held to the same rules as `of`, so a copy applied locally reads back
        // identical and isn't taken for a local edit
        let mut seen = HashSet::new();
        let variables = data
            .variables
            .into_iter()
            .filter_map(|var| {
                let key = var.key.trim().to_string();
                if key.is_empty() || !seen.insert(key.clone()) {
                    return None;
                }
                Some(SharedVar {
                    key,
                    value: if var.local {
                        None
                    } else {
                        Some(var.value.unwrap_or_default())
                    },
                    ..var
                })
            })
            .collect();
        Self {
            name: name.to_string(),
            data: SharedData { variables },
        }
    }

    pub fn from_cloud(env: &CloudEnvironment) -> Self {
        Self::from_server(&env.name, &env.data)
    }

    /// the common ancestor a link remembers
    pub fn base(link: &EnvCloudLink) -> Self {
        Self::from_server(&link.base_name, &link.base)
    }

    pub fn data_value(&self) -> Value {
        serde_json::to_value(&self.data).unwrap_or_default()
    }

    fn var(&self, key: &str) -> Option<&SharedVar> {
        self.data.variables.iter().find(|v| v.key == key)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Merged {
    pub shared: Shared,
    /// variables (and the name) both sides changed differently, ours were kept
    pub conflicts: usize,
}

/// three-way merge of `mine` and `theirs`, which both started from `base`
pub fn merge(base: &Shared, mine: &Shared, theirs: &Shared) -> Merged {
    if mine == base || mine == theirs {
        return Merged {
            shared: theirs.clone(),
            conflicts: 0,
        };
    }
    if theirs == base {
        return Merged {
            shared: mine.clone(),
            conflicts: 0,
        };
    }

    let mut conflicts = 0;
    let name = {
        let (b, m, t) = (&base.name, &mine.name, &theirs.name);
        if m == b {
            t.clone()
        } else {
            if t != b && t != m {
                conflicts += 1;
            }
            m.clone()
        }
    };

    let mut pick = |b: Option<&SharedVar>, m: Option<&SharedVar>, t: Option<&SharedVar>| {
        if m == b {
            t.cloned()
        } else if t == b || m == t {
            m.cloned()
        } else {
            conflicts += 1;
            m.cloned()
        }
    };

    // their order first, then whatever only we have, in our order
    let mut variables = Vec::new();
    let mut done = HashSet::new();
    for t in &theirs.data.variables {
        done.insert(t.key.as_str());
        if let Some(var) = pick(base.var(&t.key), mine.var(&t.key), Some(t)) {
            variables.push(var);
        }
    }
    for m in &mine.data.variables {
        if done.insert(m.key.as_str())
            && let Some(var) = pick(base.var(&m.key), Some(m), None)
        {
            variables.push(var);
        }
    }

    Merged {
        shared: Shared {
            name,
            data: SharedData { variables },
        },
        conflicts,
    }
}

/// makes `env` match `shared`, keeping this machine's values for local
/// variables. returns false (leaving `env` alone) if it already matched.
pub fn apply(env: &mut Environment, shared: &Shared) -> bool {
    if Shared::of(env) == *shared {
        return false;
    }

    let mut current: HashMap<String, String> = HashMap::new();
    for var in &env.variables {
        current
            .entry(var.key.trim().to_string())
            .or_insert_with(|| var.value.clone());
    }
    // half-typed rows without a key aren't shared, so don't lose them
    let blank: Vec<KeyValuePair> = env
        .variables
        .iter()
        .filter(|v| v.key.trim().is_empty())
        .cloned()
        .collect();

    env.name = shared.name.clone();
    env.local_keys = shared
        .data
        .variables
        .iter()
        .filter(|v| v.local)
        .map(|v| v.key.clone())
        .collect::<BTreeSet<_>>();
    env.variables = shared
        .data
        .variables
        .iter()
        .map(|v| KeyValuePair {
            is_active: v.enabled,
            key: v.key.clone(),
            value: if v.local {
                current.get(&v.key).cloned().unwrap_or_default()
            } else {
                v.value.clone().unwrap_or_default()
            },
        })
        .chain(blank)
        .collect();
    true
}

/// a new local environment from a server copy
pub fn from_cloud(env: &CloudEnvironment) -> Environment {
    let shared = Shared::from_cloud(env);
    let mut local = Environment {
        cloud: Some(EnvCloudLink {
            id: env.id.clone(),
            team_id: env.team_id.clone(),
            rev: env.rev,
            base_name: env.name.clone(),
            base: env.data.clone(),
        }),
        ..Environment::default()
    };
    apply(&mut local, &shared);
    local
}

/// one linked environment to sync, as it was when the sync started
#[derive(Debug, Clone)]
pub struct EnvJob {
    pub link: EnvCloudLink,
    pub mine: Shared,
}

#[derive(Debug, Clone)]
pub enum EnvOutcome {
    /// the server now holds `server` at `rev`; ours was merged into it
    Synced {
        rev: i64,
        server: Shared,
        conflicts: usize,
        /// we may not edit this team's environments, so our changes were dropped
        reverted: bool,
    },
    /// deleted in the cloud, or we lost access to it
    Removed,
    Failed(CloudError),
}

/// syncs every job of one team: one listing, then a push for each
/// environment that has changes the server lacks
pub async fn sync_team(
    client: &CloudClient,
    team_id: &str,
    jobs: Vec<EnvJob>,
) -> Result<Vec<(EnvJob, EnvOutcome)>, CloudError> {
    let server = match client.environments(team_id).await {
        Ok(server) => server,
        // the team was deleted or we were removed from it
        Err(err) if err.is_not_found() => {
            return Ok(jobs.into_iter().map(|j| (j, EnvOutcome::Removed)).collect());
        }
        Err(err) => return Err(err),
    };

    let mut outcomes = Vec::with_capacity(jobs.len());
    for job in jobs {
        let current = server.iter().find(|e| e.id == job.link.id);
        let outcome = sync_one(client, team_id, &job, current).await;
        if matches!(outcome, EnvOutcome::Failed(CloudError::Unauthorized)) {
            return Err(CloudError::Unauthorized);
        }
        outcomes.push((job, outcome));
    }
    Ok(outcomes)
}

async fn sync_one(
    client: &CloudClient,
    team_id: &str,
    job: &EnvJob,
    server: Option<&CloudEnvironment>,
) -> EnvOutcome {
    let synced = |env: &CloudEnvironment, conflicts| EnvOutcome::Synced {
        rev: env.rev,
        server: Shared::from_cloud(env),
        conflicts,
        reverted: false,
    };

    let mut theirs = match server {
        Some(env) => env.clone(),
        // never uploaded yet. the id is ours, so a retry after a lost
        // response finds it in the listing instead of creating it twice
        None if job.link.rev == 0 => {
            return match client
                .create_environment(
                    team_id,
                    Some(&job.link.id),
                    &job.mine.name,
                    &job.mine.data_value(),
                )
                .await
            {
                Ok(created) => synced(&created, 0),
                Err(err) => EnvOutcome::Failed(err),
            };
        }
        None => return EnvOutcome::Removed,
    };

    let base = Shared::base(&job.link);
    for _ in 0..MAX_PUSH_ROUNDS {
        let merged = merge(&base, &job.mine, &Shared::from_cloud(&theirs));
        if merged.shared == Shared::from_cloud(&theirs) {
            return synced(&theirs, merged.conflicts);
        }
        match client
            .update_environment(
                &theirs.id,
                theirs.rev,
                &merged.shared.name,
                &merged.shared.data_value(),
            )
            .await
        {
            Ok(updated) => return synced(&updated, merged.conflicts),
            // someone got there first: merge into their newer copy
            Err(CloudError::EnvironmentConflict(current)) => theirs = *current,
            Err(err) if err.is_forbidden() => {
                return EnvOutcome::Synced {
                    rev: theirs.rev,
                    server: Shared::from_cloud(&theirs),
                    conflicts: 0,
                    reverted: true,
                };
            }
            Err(err) if err.is_not_found() => return EnvOutcome::Removed,
            Err(err) => return EnvOutcome::Failed(err),
        }
    }
    EnvOutcome::Failed(CloudError::Api {
        status: 409,
        message: "the environment kept changing on the server, try again".to_string(),
    })
}
