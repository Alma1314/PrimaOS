//! Authenticated service sessions and pre-publication fork OFD reservations.
//! Cookies come from kernel metadata; payload Task IDs never select a session.
use prima_runtime::Capability;

pub const MAX_CLIENTS: usize = 18;
pub const MAX_OWNERS: usize = 32;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Client {
    pub cookie: u64,
    pub process: u64,
    pub reply: Capability,
    pub owner: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Owner {
    pub cookie: u64,
    pub parent: u64,
    pub key: u32,
}

pub struct Sessions {
    pub clients: [Option<Client>; MAX_CLIENTS],
    pub owners: [Option<Owner>; MAX_OWNERS],
    next_key: u32,
}

impl Default for Sessions {
    fn default() -> Self {
        Self::new()
    }
}
impl Sessions {
    pub const fn new() -> Self {
        Self {
            clients: [None; MAX_CLIENTS],
            owners: [None; MAX_OWNERS],
            next_key: 1,
        }
    }
    pub fn client(&self, cookie: u64) -> Option<Client> {
        self.clients
            .get(cookie as u32 as usize)
            .copied()
            .flatten()
            .filter(|c| {
                c.cookie == cookie
                    && self.owners[c.owner as usize].is_some_and(|owner| owner.cookie == c.process)
            })
    }
    pub fn owner(&self, process: u64) -> Option<u8> {
        self.owners
            .iter()
            .position(|o| o.is_some_and(|o| o.cookie != 0 && o.cookie == process))
            .map(|i| i as u8)
    }
    fn vacant_owner(&self) -> Option<u8> {
        // Slot zero remains the legacy manager-routed context.
        (1..MAX_OWNERS)
            .find(|i| self.owners[*i].is_none())
            .map(|i| i as u8)
    }
    pub fn register(
        &mut self,
        cookie: u64,
        process: u64,
        parent: u64,
        reply: Capability,
        key: u32,
    ) -> Option<u8> {
        let id = cookie as u32 as usize;
        if id == 0 || id >= MAX_CLIENTS || cookie >> 32 == 0 || process >> 32 == 0 {
            return None;
        }
        if self.clients[id].is_some_and(|old| old.cookie != cookie) {
            return None;
        }
        let owner = if let Some(owner) = self.owner(process) {
            owner
        } else if key != 0 {
            self.owners.iter().position(|o| {
                o.is_some_and(|o| o.key == key && o.parent == parent && o.cookie == 0)
            })? as u8
        } else {
            self.vacant_owner()?
        };
        let record = self.owners[owner as usize].unwrap_or(Owner {
            cookie: 0,
            parent,
            key,
        });
        self.owners[owner as usize] = Some(Owner {
            cookie: process,
            ..record
        });
        self.clients[id] = Some(Client {
            cookie,
            process,
            reply,
            owner,
        });
        Some(owner)
    }
    pub fn reserve(&mut self, parent: u64) -> Option<(u8, u32)> {
        self.owner(parent)?;
        let slot = self.vacant_owner()?;
        let key = self.next_key;
        self.next_key = key.checked_add(1)?;
        self.owners[slot as usize] = Some(Owner {
            cookie: 0,
            parent,
            key,
        });
        Some((slot, key))
    }
    pub fn publish(&mut self, parent: u64, key: u32, child: u64) -> bool {
        if child >> 32 == 0 {
            return false;
        }
        let Some(record) = self
            .owners
            .iter_mut()
            .flatten()
            .find(|o| o.parent == parent && o.key == key)
        else {
            return false;
        };
        if record.cookie != 0 && record.cookie != child {
            return false;
        }
        record.cookie = child;
        true
    }
    pub fn reservation(&self, parent: u64, key: u32) -> Option<(u8, Owner)> {
        self.owners.iter().enumerate().find_map(|(i, o)| {
            o.filter(|o| o.parent == parent && o.key == key)
                .map(|o| (i as u8, o))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn cap() -> Capability {
        Capability::from_raw((1 << 32) | 3).unwrap()
    }
    #[test]
    fn reservations_bind_to_parent_cookie_and_reused_task_ids_cannot_select_old_sessions() {
        let mut sessions = Sessions::new();
        let parent = (4 << 32) | 7;
        let child = (5 << 32) | 8;
        let owner = sessions.register(parent, parent, 0, cap(), 0).unwrap();
        let (reserved, key) = sessions.reserve(parent).unwrap();
        assert_ne!(owner, reserved);
        assert!(
            sessions
                .register(child, child, parent + (1 << 32), cap(), key)
                .is_none()
        );
        assert!(sessions.publish(parent, key, child));
        assert_eq!(
            sessions.register(child, child, parent, cap(), key),
            Some(reserved)
        );
        assert!(sessions.client(child + (1 << 32)).is_none());
        assert!(
            sessions
                .register(child + (1 << 32), child + (1 << 32), parent, cap(), 0)
                .is_none()
        );
        assert!(!sessions.publish(parent, key, child + (1 << 32)));
    }
    #[test]
    fn a_released_owner_slot_cannot_authorize_an_old_client_after_reuse() {
        let mut sessions = Sessions::new();
        let first = (4 << 32) | 7;
        let second = (5 << 32) | 8;
        let slot = sessions.register(first, first, 0, cap(), 0).unwrap();
        sessions.owners[slot as usize] = None;
        assert!(sessions.client(first).is_none());
        assert_eq!(sessions.register(second, second, 0, cap(), 0), Some(slot));
        assert!(sessions.client(first).is_none());
    }
}
