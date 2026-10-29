//! Host-only proxy-copy experiment. Arc pages represent private/COW mappings;
//! shared writable mappings, PTE allocation and TLB effects are not modeled.
use super::gate::{COPY_LIMIT, Error};
use crate::state::TaskId;
use std::{collections::BTreeMap, sync::Arc};

const PAGE: u64 = 4096;
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Page {
    pub readable: bool,
    pub writable: bool,
    pub user: bool,
    pub bytes: Arc<[u8; 4096]>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Space {
    pub identity: u64,
    pub pages: BTreeMap<u64, Page>,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Memory(pub BTreeMap<TaskId, Space>);

impl Memory {
    fn range(
        &self,
        task: TaskId,
        address: u64,
        len: usize,
        write: bool,
    ) -> Result<Vec<(u64, usize)>, Error> {
        if len > COPY_LIMIT {
            return Err(Error::Capacity);
        }
        address.checked_add(len as u64).ok_or(Error::Overflow)?;
        let space = self.0.get(&task).ok_or(Error::Fault)?;
        (0..len)
            .map(|i| {
                let addr = address + i as u64;
                let base = addr & !(PAGE - 1);
                let page = space.pages.get(&base).ok_or(Error::Fault)?;
                if !page.user || !(if write { page.writable } else { page.readable }) {
                    return Err(Error::Fault);
                }
                Ok((base, (addr - base) as usize))
            })
            .collect()
    }

    /// Explicit roots selected by TaskId. Exclusive &mut pins topology for the
    /// whole preflight/commit. Failure leaves all bytes and Arc identities intact.
    #[allow(clippy::too_many_arguments)] // both Task/root/address triples stay explicit at this boundary
    pub fn copy(
        &mut self,
        source: TaskId,
        source_identity: u64,
        src: u64,
        target: TaskId,
        target_identity: u64,
        dst: u64,
        len: usize,
    ) -> Result<(), Error> {
        if self
            .0
            .get(&source)
            .is_none_or(|s| s.identity != source_identity)
            || self
                .0
                .get(&target)
                .is_none_or(|s| s.identity != target_identity)
        {
            return Err(Error::Stale);
        }
        let input = self.range(source, src, len, false)?;
        let output = self.range(target, dst, len, true)?;
        let bytes: Vec<_> = input
            .iter()
            .map(|&(base, offset)| self.0[&source].pages[&base].bytes[offset])
            .collect();
        for ((base, offset), value) in output.into_iter().zip(bytes) {
            let page = self
                .0
                .get_mut(&target)
                .unwrap()
                .pages
                .get_mut(&base)
                .unwrap();
            Arc::make_mut(&mut page.bytes)[offset] = value;
        }
        Ok(())
    }
}
