//! SPEC-081：`COSIR 原语规范.md` §3.1 的 `Path` 与 §5.4 的 `NsEntry`。
//!
//! 阶段 3 第一片（切片 AJ）把 Namespace 从「只物化一个空 init Namespace」推进到 §11 的七个
//! 路径操作。本模块只承载**数据类型与路径语法**（纯函数、可 host 单测）；操作语义在
//! `bootstrap.rs` 的 `BootstrapState::ns_*` 里。
//!
//! `Path` 是 `Copy` 的定长值类型（不分配），故可直接存进 arena 条目、审计事件与事件见证。

use crate::handle::Handle;

/// 单个分量的字节上限。
///
/// 规范 §3.1 允许 ≤ 255 字节；本细化层取 16——登记为细化层约束偏差（规范只要求「不得超过」，
/// 未要求取到最大值），见设计文档 §0.1。
pub const MAX_COMPONENT_LEN: usize = 16;
/// 路径的分量数上限（§3.1 约束 4 的 `MAX_PATH_COMPONENTS`）。规范未给值，本层取 4。
pub const MAX_PATH_COMPONENTS: usize = 4;
/// 规范化文本（`render`）的字节上限：每个分量最多 `MAX_COMPONENT_LEN` 字节 + 一个 `/`。
pub const MAX_RENDERED_PATH_LEN: usize = MAX_PATH_COMPONENTS * (MAX_COMPONENT_LEN + 1);

/// `Path::parse` 的失败域（§3.1 的逐条约束）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PathError {
    /// 外部 Namespace 操作的 `Path` 必须非空（§3.1 约束 2）；空列表只在内部表示命名空间根。
    Empty,
    /// 存在空分量：前导 `/`、尾随 `/` 或 `//`（§3.1 约束 3）。
    EmptyComponent,
    /// 分量超过 [`MAX_COMPONENT_LEN`]（规范允许 ≤ 255）。
    ComponentTooLong,
    /// 分量数超过 [`MAX_PATH_COMPONENTS`]。
    TooManyComponents,
    /// `.` 或 `..` 不能作为分量（§3.1 约束 1）。
    ReservedComponent,
    /// 分量含非 ASCII 字节（§3.1 要求 ASCII）。
    NonAsciiComponent,
}

/// §3.1 的一个 `Component`：非空 ASCII 字节串，不含 `/`。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Component {
    len: u8,
    bytes: [u8; MAX_COMPONENT_LEN],
}

impl Component {
    /// 空分量（仅用于构造紧凑的 `Path` 初值；合法的 `Path` 不含空分量）。
    pub const EMPTY: Self = Self {
        len: 0,
        bytes: [0; MAX_COMPONENT_LEN],
    };

    /// 从字节串构造（调用方已保证 ≤ `MAX_COMPONENT_LEN`）。
    fn from_bytes(bytes: &[u8]) -> Self {
        let mut component = Self::EMPTY;
        let mut index = 0;
        while index < bytes.len() {
            component.bytes[index] = bytes[index];
            index += 1;
        }
        component.len = bytes.len() as u8;
        component
    }

    /// 分量字节。
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes[..self.len as usize]
    }

    /// 分量长度（字节）。
    pub const fn len(&self) -> usize {
        self.len as usize
    }

    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// 与另一分量是否逐字节相等。
    pub fn eq_bytes(&self, other: &[u8]) -> bool {
        self.len as usize == other.len() && self.as_bytes() == other
    }

    /// 是否为保留分量（`.` 或 `..`）。
    fn is_reserved(&self) -> bool {
        self.eq_bytes(b".") || self.eq_bytes(b"..")
    }
}

/// §3.1 的一个 `Path`：分量列表。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Path {
    len: u8,
    components: [Component; MAX_PATH_COMPONENTS],
}

impl Path {
    /// 空路径（规范内部表示命名空间根；外部操作不可传空路径，见 [`Path::parse`]）。
    pub const ROOT: Self = Self {
        len: 0,
        components: [Component::EMPTY; MAX_PATH_COMPONENTS],
    };

    /// 解析外部传入的路径字节串（§3.1 的全部约束）。
    pub fn parse(bytes: &[u8]) -> Result<Self, PathError> {
        if bytes.is_empty() {
            return Err(PathError::Empty);
        }
        let mut path = Self::ROOT;
        let mut start = 0usize;
        let mut index = 0usize;
        // 结尾补一个分隔符，使循环内统一处理最后一个分量。
        while index <= bytes.len() {
            let at_end = index == bytes.len();
            if at_end || bytes[index] == b'/' {
                let component = &bytes[start..index];
                if component.is_empty() {
                    // 前导 `/`、尾随 `/` 或 `//` 都落在这里。
                    return Err(PathError::EmptyComponent);
                }
                if path.len as usize == MAX_PATH_COMPONENTS {
                    return Err(PathError::TooManyComponents);
                }
                if component.len() > MAX_COMPONENT_LEN {
                    return Err(PathError::ComponentTooLong);
                }
                let mut byte_index = 0;
                while byte_index < component.len() {
                    if component[byte_index] > 0x7f {
                        return Err(PathError::NonAsciiComponent);
                    }
                    byte_index += 1;
                }
                let parsed = Component::from_bytes(component);
                if parsed.is_reserved() {
                    return Err(PathError::ReservedComponent);
                }
                path.components[path.len as usize] = parsed;
                path.len += 1;
                start = index + 1;
            }
            index += 1;
        }
        Ok(path)
    }

    /// 分量数。
    pub const fn len(&self) -> usize {
        self.len as usize
    }

    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// 第 `index` 个分量（越界返回 `None`）。
    pub const fn component(&self, index: usize) -> Option<Component> {
        if index < self.len as usize {
            Some(self.components[index])
        } else {
            None
        }
    }

    /// 最后一个分量（空路径返回 `None`）。
    pub const fn last(&self) -> Option<Component> {
        if self.len == 0 {
            None
        } else {
            Some(self.components[self.len as usize - 1])
        }
    }

    /// `self` 是否为 `other` 的前缀（含相等）；空路径是任意路径的前缀（命名空间根）。
    pub fn is_prefix_of(&self, other: &Self) -> bool {
        if self.len() > other.len() {
            return false;
        }
        let mut index = 0;
        while index < self.len() {
            if self.components[index] != other.components[index] {
                return false;
            }
            index += 1;
        }
        true
    }

    /// `self` 是否为 `other` 的**严格**前缀（§11.1 约束 3 用的判定）。
    pub fn strict_prefix_of(&self, other: &Self) -> bool {
        self.len() < other.len() && self.is_prefix_of(other)
    }

    /// 去掉最后一个分量；根路径返回 `None`。
    pub fn parent(&self) -> Option<Self> {
        if self.len == 0 {
            return None;
        }
        let mut path = *self;
        path.components[path.len as usize - 1] = Component::EMPTY;
        path.len -= 1;
        Some(path)
    }

    /// 追加一个分量（超出上限返回 `Err`）。
    pub fn extend(&self, component: &[u8]) -> Result<Self, PathError> {
        if self.len as usize == MAX_PATH_COMPONENTS {
            return Err(PathError::TooManyComponents);
        }
        if component.is_empty() {
            return Err(PathError::EmptyComponent);
        }
        if component.len() > MAX_COMPONENT_LEN {
            return Err(PathError::ComponentTooLong);
        }
        let mut path = *self;
        let parsed = Component::from_bytes(component);
        if parsed.is_reserved() {
            return Err(PathError::ReservedComponent);
        }
        path.components[path.len as usize] = parsed;
        path.len += 1;
        Ok(path)
    }

    /// 取从第 `start` 个分量起的尾部（供 `ns_resolve` 消费路径）。
    pub fn suffix_from(&self, start: usize) -> Self {
        let mut path = Self::ROOT;
        let mut index = start;
        while index < self.len as usize {
            path.components[path.len as usize] = self.components[index];
            path.len += 1;
            index += 1;
        }
        path
    }

    /// 规范化文本（分量以 `/` 连接，形如 `a/b`；根路径为 `""`），供事件与审计。
    ///
    /// 按 §3.1 约束 3，文本形式**不含**前导/尾随 `/`（那会产生空分量）。
    pub fn render(&self) -> RenderedPath {
        let mut rendered = RenderedPath {
            len: 0,
            bytes: [0; MAX_RENDERED_PATH_LEN],
        };
        let mut index = 0;
        while index < self.len as usize {
            if index > 0 {
                rendered.bytes[rendered.len as usize] = b'/';
                rendered.len += 1;
            }
            let component = &self.components[index];
            let mut byte_index = 0;
            while byte_index < component.len() {
                rendered.bytes[rendered.len as usize] = component.bytes[byte_index];
                rendered.len += 1;
                byte_index += 1;
            }
            index += 1;
        }
        rendered
    }
}

/// [`Path::render`] 的结果：定长字节缓冲，`no_std` 下无 `String`。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RenderedPath {
    len: u8,
    bytes: [u8; MAX_RENDERED_PATH_LEN],
}

impl RenderedPath {
    /// 规范化文本的字节。
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes[..self.len as usize]
    }

    pub const fn len(&self) -> usize {
        self.len as usize
    }

    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// 与另一字节串是否逐字节相等（测试与断言用）。
    pub fn eq_bytes(&self, other: &[u8]) -> bool {
        self.len as usize == other.len() && self.as_bytes() == other
    }
}

/// §5.4 的 `NsEntry` 判别：本地绑定项或挂载项。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NsEntryKind {
    Bound,
    Mount,
}

/// §5.4 的一个 Namespace 条目。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NsEntry {
    /// 所属 Namespace 的对象 id。
    pub ns: u32,
    /// 条目在所属 Namespace 内的路径。
    pub path: Path,
    pub kind: NsEntryKind,
    /// `Bound`/`Mount` 指向的 Capability。
    pub cap: Handle,
    /// 建立该条目的调用者（`ns_bind`/`ns_mount` 的 caller）。
    pub creator: u32,
    /// `Mount` 条目的**目标 Namespace id**（`Bound` 为 0）。
    ///
    /// 单独记一份而非每次经 cap 反查，是为了让 `ns_mount` 的环检测（§11.1 约束 6）只依赖
    /// 命名空间图的边——即使挂载 Capability 变 stale、源 Namespace 已销毁，图的形状仍然可知。
    pub target_ns: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(text: &str) -> Result<Path, PathError> {
        Path::parse(text.as_bytes())
    }

    #[test]
    fn parses_a_single_component() {
        let path = parse("lib").expect("valid path");
        assert_eq!(path.len(), 1);
        assert!(path.component(0).is_some_and(|c| c.eq_bytes(b"lib")));
    }

    #[test]
    fn parses_multiple_components_in_order() {
        let path = parse("a/b/c").expect("valid path");
        assert_eq!(path.len(), 3);
        assert!(path.component(0).is_some_and(|c| c.eq_bytes(b"a")));
        assert!(path.component(1).is_some_and(|c| c.eq_bytes(b"b")));
        assert!(path.component(2).is_some_and(|c| c.eq_bytes(b"c")));
        assert!(path.component(3).is_none());
        assert!(path.last().is_some_and(|c| c.eq_bytes(b"c")));
    }

    #[test]
    fn rejects_empty_path() {
        assert_eq!(parse(""), Err(PathError::Empty));
        assert!(Path::ROOT.is_empty());
        assert!(Path::ROOT.last().is_none());
    }

    #[test]
    fn rejects_leading_trailing_and_double_slashes() {
        assert_eq!(parse("/a"), Err(PathError::EmptyComponent));
        assert_eq!(parse("a/"), Err(PathError::EmptyComponent));
        assert_eq!(parse("a//b"), Err(PathError::EmptyComponent));
        assert_eq!(parse("/"), Err(PathError::EmptyComponent));
    }

    #[test]
    fn rejects_dot_and_dotdot_components() {
        assert_eq!(parse("."), Err(PathError::ReservedComponent));
        assert_eq!(parse(".."), Err(PathError::ReservedComponent));
        assert_eq!(parse("a/../b"), Err(PathError::ReservedComponent));
    }

    #[test]
    fn rejects_components_longer_than_the_limit() {
        let long = "x".repeat(MAX_COMPONENT_LEN + 1);
        assert_eq!(parse(&long), Err(PathError::ComponentTooLong));
        let at_limit = "y".repeat(MAX_COMPONENT_LEN);
        assert!(parse(&at_limit).is_ok());
    }

    #[test]
    fn rejects_too_many_components() {
        assert!(parse("a/b/c/d").is_ok());
        assert_eq!(parse("a/b/c/d/e"), Err(PathError::TooManyComponents));
    }

    #[test]
    fn rejects_non_ascii_components() {
        assert_eq!(
            Path::parse("\u{e9}".as_bytes()),
            Err(PathError::NonAsciiComponent)
        );
        assert_eq!(Path::parse(&[0x80]), Err(PathError::NonAsciiComponent));
        assert_eq!(
            Path::parse(b"a/\x7f"),
            Ok(parse("a/\x7f").expect("ascii is allowed"))
        );
    }

    #[test]
    fn prefix_relations_including_equality_and_root() {
        let root = Path::ROOT;
        let a = parse("a").expect("valid");
        let ab = parse("a/b").expect("valid");
        let ac = parse("a/c").expect("valid");
        let not_a_child = parse("ab").expect("valid");
        assert!(root.is_prefix_of(&a));
        assert!(root.is_prefix_of(&root));
        assert!(a.is_prefix_of(&ab));
        assert!(a.is_prefix_of(&ac));
        assert!(a.is_prefix_of(&a));
        assert!(!a.is_prefix_of(&not_a_child));
        assert!(!ab.is_prefix_of(&a));
        assert!(!ab.is_prefix_of(&ac));
        assert!(a.strict_prefix_of(&ab));
        assert!(!a.strict_prefix_of(&a));
        assert!(!root.strict_prefix_of(&root));
    }

    #[test]
    fn parent_and_suffix() {
        let abc = parse("a/b/c").expect("valid");
        let ab = abc.parent().expect("has parent");
        assert_eq!(ab.render().as_bytes(), b"a/b");
        let a = ab.parent().expect("has parent");
        assert_eq!(a.render().as_bytes(), b"a");
        // 单分量路径的父项是空路径（§11.7 的「当前前缀」可以为根），而不是 None。
        assert_eq!(a.parent(), Some(Path::ROOT));
        assert!(Path::ROOT.parent().is_none());

        assert_eq!(abc.suffix_from(1).render().as_bytes(), b"b/c");
        assert_eq!(abc.suffix_from(3).render().as_bytes(), b"");
        assert_eq!(abc.suffix_from(0).render().as_bytes(), b"a/b/c");
    }

    #[test]
    fn extend_builds_paths_and_enforces_limits() {
        let a = parse("a").expect("valid");
        let ab = a.extend(b"b").expect("room");
        assert_eq!(ab.render().as_bytes(), b"a/b");
        assert_eq!(Path::ROOT.extend(b"x").map(|p| p.len()), Ok(1));
        assert_eq!(a.extend(b""), Err(PathError::EmptyComponent));
        assert_eq!(a.extend(b".."), Err(PathError::ReservedComponent));
        let long = "z".repeat(MAX_COMPONENT_LEN + 1);
        assert_eq!(a.extend(long.as_bytes()), Err(PathError::ComponentTooLong));

        let full = parse("a/b/c/d").expect("valid");
        assert_eq!(full.extend(b"e"), Err(PathError::TooManyComponents));
    }

    #[test]
    fn render_joins_components_without_leading_or_trailing_slash() {
        assert!(Path::ROOT.render().is_empty());
        assert_eq!(parse("lib").expect("valid").render().as_bytes(), b"lib");
        assert_eq!(parse("a/b").expect("valid").render().as_bytes(), b"a/b");
        assert_eq!(
            parse("a/b/c/d").expect("valid").render().as_bytes(),
            b"a/b/c/d"
        );
        // 渲染结果不超过缓冲上限（含最大分量与最大深度）。
        let deepest = parse("aaaaaaaaaaaaaaaa/bbbbbbbbbbbbbbbb/cccccccccccccccc/dddddddddddddddd")
            .expect("valid");
        assert!(deepest.render().len() <= MAX_RENDERED_PATH_LEN);
        assert!(
            deepest
                .render()
                .eq_bytes(b"aaaaaaaaaaaaaaaa/bbbbbbbbbbbbbbbb/cccccccccccccccc/dddddddddddddddd")
        );
    }

    #[test]
    fn parsed_paths_are_equal_by_value() {
        assert_eq!(parse("a/b").expect("valid"), parse("a/b").expect("valid"));
        assert_ne!(parse("a/b").expect("valid"), parse("a/c").expect("valid"));
    }
}
