//! 权限位集（COSIR §4.1）。
//!
//! 规范要求 `rights: FiniteSet<Right>`，且 `well_typed_rights(target, rights)` 要求权限与目标
//! 对象类型匹配（`Derive` 例外，可用于任何对象类型）。本模块是纯逻辑、无 asm，唯一权限宿主，
//! 供 host 单测钉死；内核其余部分不再用逗号分隔字符串表示权限。
//!
//! 先前实现用字符串 `"all"` 表示 init Namespace 的完整权限，既非有限集合也无法做类型匹配，
//! 本模块将其替换为规范列出的有限集。

/// §4.1 的全部合法 Right。
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Right {
    Derive,
    SegMapR,
    SegMapW,
    SegMapX,
    SegDestroy,
    TaskSend,
    TaskManageVSpace,
    TaskDestroy,
    NsResolve,
    NsBind,
    NsUnbind,
    NsMount,
    NsUnmount,
    NsDestroy,
    TimerCancel,
    TimerDestroy,
    ExcSubscribe,
    ExcSourceDestroy,
    NodeConnect,
    NodeDestroy,
    /// §15.3 `DeviceRight` 的 **IOPortRange** 子集（MMIO/DMA/IRQ 的权利在实现对应设备种类时再加）。
    DevIORead,
    DevIOWrite,
    DevDestroy,
}

/// 全部 Right 的规范枚举顺序（同时是渲染顺序与解析顺序）。
pub const ALL_RIGHTS: [Right; 23] = [
    Right::Derive,
    Right::SegMapR,
    Right::SegMapW,
    Right::SegMapX,
    Right::SegDestroy,
    Right::TaskSend,
    Right::TaskManageVSpace,
    Right::TaskDestroy,
    Right::NsResolve,
    Right::NsBind,
    Right::NsUnbind,
    Right::NsMount,
    Right::NsUnmount,
    Right::NsDestroy,
    Right::TimerCancel,
    Right::TimerDestroy,
    Right::ExcSubscribe,
    Right::ExcSourceDestroy,
    Right::NodeConnect,
    Right::NodeDestroy,
    Right::DevIORead,
    Right::DevIOWrite,
    Right::DevDestroy,
];

impl Right {
    pub const fn name(self) -> &'static str {
        match self {
            Right::Derive => "Derive",
            Right::SegMapR => "SegMapR",
            Right::SegMapW => "SegMapW",
            Right::SegMapX => "SegMapX",
            Right::SegDestroy => "SegDestroy",
            Right::TaskSend => "TaskSend",
            Right::TaskManageVSpace => "TaskManageVSpace",
            Right::TaskDestroy => "TaskDestroy",
            Right::NsResolve => "NsResolve",
            Right::NsBind => "NsBind",
            Right::NsUnbind => "NsUnbind",
            Right::NsMount => "NsMount",
            Right::NsUnmount => "NsUnmount",
            Right::NsDestroy => "NsDestroy",
            Right::TimerCancel => "TimerCancel",
            Right::TimerDestroy => "TimerDestroy",
            Right::ExcSubscribe => "ExcSubscribe",
            Right::ExcSourceDestroy => "ExcSourceDestroy",
            Right::NodeConnect => "NodeConnect",
            Right::NodeDestroy => "NodeDestroy",
            Right::DevIORead => "DevIORead",
            Right::DevIOWrite => "DevIOWrite",
            Right::DevDestroy => "DevDestroy",
        }
    }

    const fn bit(self) -> u32 {
        1 << (self as u32)
    }

    fn parse(name: &str) -> Option<Self> {
        ALL_RIGHTS.into_iter().find(|right| right.name() == name)
    }
}

/// Capability 目标的对象类型（§4.1 的类型匹配表）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ObjectKind {
    Segment,
    Task,
    Namespace,
    Timer,
    ExcSource,
    Node,
    /// §15.2 `Device(DevId)`——设备对象（本片仅实现 `IOPortRange` 种）。
    Device,
}

impl ObjectKind {
    /// 该对象类型允许的**非 Derive** 权限。
    const fn allowed(self) -> &'static [Right] {
        match self {
            ObjectKind::Segment => &[
                Right::SegMapR,
                Right::SegMapW,
                Right::SegMapX,
                Right::SegDestroy,
            ],
            ObjectKind::Task => &[Right::TaskSend, Right::TaskManageVSpace, Right::TaskDestroy],
            ObjectKind::Namespace => &[
                Right::NsResolve,
                Right::NsBind,
                Right::NsUnbind,
                Right::NsMount,
                Right::NsUnmount,
                Right::NsDestroy,
            ],
            ObjectKind::Timer => &[Right::TimerCancel, Right::TimerDestroy],
            ObjectKind::ExcSource => &[Right::ExcSubscribe, Right::ExcSourceDestroy],
            ObjectKind::Node => &[Right::NodeConnect, Right::NodeDestroy],
            ObjectKind::Device => &[Right::DevIORead, Right::DevIOWrite, Right::DevDestroy],
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RightsError {
    UnknownRight,
    EmptyName,
}

/// 权限有限集合（位集）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rights(u32);

impl Rights {
    pub const EMPTY: Self = Self(0);

    /// 由显式 Right 列表构造（const 上下文可用，供初始 Authority 常量使用）。
    pub const fn from_rights(rights: &[Right]) -> Self {
        let mut bits = 0u32;
        let mut index = 0;
        while index < rights.len() {
            bits |= rights[index].bit();
            index += 1;
        }
        Self(bits)
    }

    /// 原始位数（仅供跨 crate 镜像常量与测试使用）。
    pub const fn bits(self) -> u32 {
        self.0
    }

    /// 由原始位集还原（syscall ABI 的 `cap_derive` 请求权限），未定义位被保留原样，
    /// 由后续 `well_typed` / `subset_of` 判定拒绝。
    pub const fn from_bits(bits: u32) -> Self {
        Self(bits)
    }

    pub const fn contains(self, right: Right) -> bool {
        self.0 & right.bit() != 0
    }

    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    pub const fn count(self) -> u32 {
        self.0.count_ones()
    }

    /// 位集超集判定（`self ⊇ other`）。
    pub const fn superset_of(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    /// 子集判定（`self ⊆ other`），派生时用 `child.subset_of(parent)`。
    pub const fn subset_of(self, other: Self) -> bool {
        other.superset_of(self)
    }

    /// 交集（用于错误信息与测试）。
    pub const fn intersection(self, other: Self) -> Self {
        Self(self.0 & other.0)
    }

    /// 并集（铸造根 Capability 时合成权限）。
    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    /// 类型匹配：非 `Derive` 权限必须落在该对象类型的合法集合内（`Derive` 可用于任何类型）。
    pub fn well_typed(self, kind: ObjectKind) -> bool {
        let allowed = kind.allowed();
        ALL_RIGHTS
            .into_iter()
            .filter(|right| self.contains(*right))
            .all(|right| right == Right::Derive || allowed.contains(&right))
    }

    /// 解析逗号分隔的权限名。空白会被忽略；空名与未知名报错。
    pub fn parse(text: &str) -> Result<Self, RightsError> {
        let mut bits = 0u32;
        for part in text.split(',') {
            let name = part.trim();
            if name.is_empty() {
                return Err(RightsError::EmptyName);
            }
            let right = Right::parse(name).ok_or(RightsError::UnknownRight)?;
            bits |= right.bit();
        }
        Ok(Self(bits))
    }

    /// 按 [`ALL_RIGHTS`] 顺序渲染到调用方提供的缓冲，返回其中的 `&str`。
    ///
    /// 缓冲不足时返回 `None`（调用方提供 ≥ 256 字节即可，本模块不做分配）。
    pub fn render(self, buffer: &mut [u8]) -> Option<&str> {
        let mut length = 0usize;
        let mut first = true;
        for right in ALL_RIGHTS {
            if !self.contains(right) {
                continue;
            }
            if !first {
                if length == buffer.len() {
                    return None;
                }
                buffer[length] = b',';
                length += 1;
            }
            first = false;
            let name = right.name().as_bytes();
            if length + name.len() > buffer.len() {
                return None;
            }
            buffer[length..length + name.len()].copy_from_slice(name);
            length += name.len();
        }
        core::str::from_utf8(&buffer[..length]).ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(text: &str) -> Rights {
        Rights::parse(text).expect("rights text parses")
    }

    fn render(rights: Rights) -> String {
        let mut buffer = [0u8; 256];
        rights
            .render(&mut buffer)
            .expect("buffer is large enough")
            .to_owned()
    }

    #[test]
    fn parses_and_renders_in_canonical_order() {
        let rights = parse("SegDestroy,Derive,SegMapR");
        assert_eq!(render(rights), "Derive,SegMapR,SegDestroy");
        assert!(rights.contains(Right::Derive));
        assert!(rights.contains(Right::SegMapR));
        assert!(!rights.contains(Right::SegMapW));
    }

    #[test]
    fn initial_authority_rights_match_spec_4_1() {
        assert_eq!(
            render(parse("Derive,SegMapR,SegMapX,SegDestroy")),
            "Derive,SegMapR,SegMapX,SegDestroy"
        );
        assert_eq!(
            render(parse(
                "Derive,NsResolve,NsBind,NsUnbind,NsMount,NsUnmount,NsDestroy"
            )),
            "Derive,NsResolve,NsBind,NsUnbind,NsMount,NsUnmount,NsDestroy"
        );
        assert_eq!(
            render(parse("Derive,TaskSend,TaskManageVSpace,TaskDestroy")),
            "Derive,TaskSend,TaskManageVSpace,TaskDestroy"
        );
        assert_eq!(
            render(parse("Derive,ExcSubscribe,ExcSourceDestroy")),
            "Derive,ExcSubscribe,ExcSourceDestroy"
        );
    }

    #[test]
    fn rejects_unknown_and_empty_names() {
        assert_eq!(Rights::parse("Derive,Nope"), Err(RightsError::UnknownRight));
        assert_eq!(
            Rights::parse("Derive,,SegMapR"),
            Err(RightsError::EmptyName)
        );
        assert_eq!(Rights::parse(""), Err(RightsError::EmptyName));
    }

    #[test]
    fn wildcard_is_not_a_right() {
        // 先前实现的 `"all"` 通配符不是规范权限，必须被拒绝。
        assert_eq!(Rights::parse("all"), Err(RightsError::UnknownRight));
    }

    #[test]
    fn subset_and_superset_track_derivation_monotonicity() {
        let parent = parse("Derive,SegMapR,SegMapW");
        let child = parse("SegMapR");
        assert!(child.subset_of(parent));
        assert!(parent.superset_of(child));
        assert!(!parent.subset_of(child));
        assert_eq!(render(parent.intersection(child)), "SegMapR");
    }

    #[test]
    fn well_typed_matches_the_object_kind_table() {
        assert!(parse("Derive,SegMapR,SegDestroy").well_typed(ObjectKind::Segment));
        assert!(!parse("TaskSend").well_typed(ObjectKind::Segment));
        assert!(parse("Derive,TaskDestroy").well_typed(ObjectKind::Task));
        assert!(!parse("NsResolve").well_typed(ObjectKind::Task));
        assert!(parse("Derive,NsMount,NsResolve").well_typed(ObjectKind::Namespace));
        assert!(parse("ExcSubscribe").well_typed(ObjectKind::ExcSource));
        assert!(!parse("ExcSubscribe").well_typed(ObjectKind::Namespace));
        // `Derive` 自身可用于任何对象类型。
        assert!(parse("Derive").well_typed(ObjectKind::Node));
    }

    #[test]
    fn device_ioport_rights_are_typed_to_devices_only() {
        // §15.5：`dev_ioport_create` 的根 cap 权限 = {Derive, DevIORead, DevIOWrite, DevDestroy}。
        let ioport = parse("DevIOWrite,Derive,DevIORead,DevDestroy");
        assert_eq!(render(ioport), "Derive,DevIORead,DevIOWrite,DevDestroy");
        assert!(ioport.well_typed(ObjectKind::Device));
        // 设备权利不得落在其它对象类型上，反之亦然。
        assert!(!parse("DevIORead").well_typed(ObjectKind::Segment));
        assert!(!parse("SegMapR").well_typed(ObjectKind::Device));
        assert!(parse("Derive").well_typed(ObjectKind::Device));
    }

    #[test]
    fn render_reports_small_buffers() {
        let rights = parse("Derive,SegMapR,SegMapX,SegDestroy");
        let mut small = [0u8; 8];
        assert!(rights.render(&mut small).is_none());
    }

    #[test]
    fn empty_rights_are_empty_and_not_well_typed_violating() {
        assert!(Rights::EMPTY.is_empty());
        assert_eq!(Rights::EMPTY.count(), 0);
        assert!(Rights::EMPTY.well_typed(ObjectKind::Segment));
    }
}
