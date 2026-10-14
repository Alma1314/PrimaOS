//! 服务 wire protocol 的版本和 feature 协商。

/// 消息头固定长度。各字段显式按 little-endian 编码，不依赖 Rust 结构体布局。
pub const MESSAGE_HEADER_LEN: usize = 20;
/// Hello offer 固定长度。
pub const OFFER_LEN: usize = 24;
const MAGIC: [u8; 4] = *b"PRIM";

/// SPEC-103：ring3 服务管理器向 init **自报存活**的消息 tag（高 16 位）；低 8 位为自身 task id。
/// 与内核 `user::SERVICEMGR_HELLO_TAG` 同款。
pub const SERVICEMGR_HELLO_TAG: u64 = 0x5E56_0000_0000_0000;

/// SPEC-104：端点/客户端向服务管理器发送**带 cap 消息之前**先发的**无 cap 唤醒** tag。
///
/// 内核拒绝向处于 `BlockedRecv` 的接收者投递带 cap 消息（`kernel/src/syscall.rs::send_call`），
/// 而服务管理器常态阻塞在 `recv`，故发送方先发一条无 cap 消息直投唤醒它，再发带 cap 消息。
/// 该 tag 不被任何接收分支匹配，接收方忽略之。
pub const SERVICE_WAKE_TAG: u64 = 0x5ECA_0000_0000_0000;

/// SPEC-105：`init` 向服务管理器移交**建端点所需 cap 与参数**的消息 tag。
///
/// payload 布局：`tag(48..63) | payload_bits(8..47) | subtype(0..7)`；subtype 见
/// [`SERVICE_SETUP_CODE`]/[`SERVICE_SETUP_STACK`]。与端点自报/完成 tag 不相交。
pub const SERVICE_SETUP_TAG: u64 = 0x5ECC_0000_0000_0000;
/// SPEC-108：`init` 向服务管理器移交**服务代码段**的消息 tag。
///
/// payload 布局与 [`SERVICE_SETUP_TAG`] 相同（`code_pages(40..47) | name(0..39)`），但带
/// **一条** cap（cap0 = 代码段）。服务**栈段**不再移交——由管理器 `SYS_MEMSEG_CREATE` 自建。
/// 与 [`SERVICE_SETUP_TAG`]（父 Namespace，仅 tag 无 payload 位）以高 16 位区分。
pub const SERVICE_SETUP_CODE_TAG: u64 = 0x5ECD_0000_0000_0000;
/// SPEC-105：subtype——本消息携带某服务的**代码段 cap**（另一条 cap 槽留给栈段前一条）。
/// `payload_bits` 为 `code_pages`（1..=255）。
pub const SERVICE_SETUP_CODE: u64 = 1;
/// SPEC-105：subtype——本消息携带某服务的**栈段 cap**；`payload_bits` 为该服务**注册名**。
pub const SERVICE_SETUP_STACK: u64 = 2;
/// SPEC-105：端点在服务管理器根内的**代码探测 VA**：服务管理器把代码段映射到此以读 ELF 头
/// （`e_entry` 在偏移 24..32，未被装载器覆写）取入口，随后卸载。
pub const ENDPOINT_CODE_PROBE_VA: u64 = 0x0000_0A00_0000_8000;
/// SPEC-105：端点的代码基址（`task_create` 的 `load_base`；入口 = 此 + `e_entry`）。
pub const ENDPOINT_CODE_VA: u64 = 0x0000_0B00_0000_0000;
/// SPEC-105：端点的用户栈基址（单页；`stack_top = 此 + 0x1000`）。须等于内核
/// `user::USER3_STACK_VA`（`USER3_BASE_VA + 0x10_0000`），否则新端点的 `rsp` 落错。
pub const ENDPOINT_STACK_VA: u64 = 0x0000_0B00_0010_0000;
/// SPEC-105：ELF 头 `e_entry` 的偏移。
pub const ELF_ENTRY_OFFSET: u64 = 24;

/// SPEC-109：内核为 ring3 服务管理器打包的**服务映像段**目录页魔数（段**首页**首个 u64）。
/// 与内核 `user::SERVICE_IMAGE_MAGIC` 同款。
pub const SERVICE_IMAGE_MAGIC: u64 = 0x5345_5256_494D_4701;
/// SPEC-109：目录头部字节数（`magic(0..8) | count(8..16)`）。
pub const SERVICE_IMAGE_DIR_HEADER: u64 = 16;
/// SPEC-109：单个目录项字节数（`name(0..8) | offset_pages(8..16) | page_count(16..24)`）。
pub const SERVICE_IMAGE_ENTRY_LEN: u64 = 24;

/// SPEC-090：客户端在共享段内写入**请求服务名**的偏移（位于 20 B 头 + 24 B offer 之后）。
/// 内核侧 init（服务管理器）从同一物理帧的映射读回该名字以解析端点。
pub const SERVICE_NAME_OFFSET: u64 = 48;
/// SPEC-101：服务名是**最多 5 字节的 ASCII 令牌**，按 little-endian 打包进一个 u64 的低 40 位
/// （尾部补 `0`）。取代 SPEC-095 的 64 位数字码——名字从此是字符串而非魔数。
pub const SERVICE_NAME_MAX: usize = 5;
/// SPEC-101：有效名字位掩码（低 40 位）。
pub const SERVICE_NAME_MASK: u64 = 0x0000_00ff_ffff_ffff;

/// SPEC-101：把 ASCII 令牌打包为服务名（超过 5 字节截断）。与内核 `user::service_name` 同款。
pub const fn pack_service_name(bytes: &[u8]) -> u64 {
    let mut packed = 0u64;
    let mut index = 0;
    while index < bytes.len() && index < SERVICE_NAME_MAX {
        packed |= (bytes[index] as u64) << (8 * index);
        index += 1;
    }
    packed
}

/// SPEC-101：`ALPHA` 端点的服务名 `"alpha"`（打包值 `0x6168_706c_61`）。
pub const SERVICE_NAME_ALPHA: u64 = pack_service_name(b"alpha");
/// SPEC-101：`BETA` 端点的服务名 `"beta"`（打包值 `0x6174_6562`）。被服务管理器解析为行为码 2；
/// 未注册名解析为 0（拒绝）。
pub const SERVICE_NAME_BETA: u64 = pack_service_name(b"beta");
/// SPEC-106（S4-D3）：三个基础服务的注册名——`"log"`/`"nsvc"`/`"store"`（各 ≤5 字节，打包进 u64 低 40 位）。
pub const SERVICE_NAME_LOG: u64 = pack_service_name(b"log");
pub const SERVICE_NAME_NS: u64 = pack_service_name(b"nsvc");
pub const SERVICE_NAME_STORE: u64 = pack_service_name(b"store");

/// SPEC-106：基础服务向服务管理器自报**编译进二进制的存在标记**的消息 tag。
///
/// payload 布局：`tag(48..63) | kind(40..47) | marker(0..39)`。`kind` 是服务的行为码
/// （[`SERVICE_BEHAVIOR_LOG`]/[`SERVICE_BEHAVIOR_NS`]/[`SERVICE_BEHAVIOR_STORE`]），供管理器把
/// 标记归到正确的注册表槽；`marker` 是**每个二进制相异**的 40 位常量（[`SERVICE_MARKER_LOG`] 等）。
/// 内核据此逐字段核验「三个服务各由**不同二进制**运行」。
pub const SERVICE_MARKER_TAG: u64 = 0x5EC6_0000_0000_0000;
/// SPEC-106：marker 字段的位掩码（低 40 位）。
pub const SERVICE_MARKER_MASK: u64 = 0x0000_00ff_ffff_ffff;
/// SPEC-106：`kind` 字段在 marker 消息 payload 中的左移位数。
pub const SERVICE_MARKER_KIND_SHIFT: u64 = 40;
/// SPEC-106：三个基础服务的 40 位存在标记（互异，且与 `ALPHA`/`BETA` 的 64 位标记风格不相交）。
pub const SERVICE_MARKER_LOG: u64 = 0x0000_004c_4f47_0001;
pub const SERVICE_MARKER_NS: u64 = 0x0000_004e_5356_0002;
pub const SERVICE_MARKER_STORE: u64 = 0x0000_0053_5452_0003;

/// SPEC-106：把存在标记打包成 marker 消息 payload。
pub const fn pack_service_marker(kind: u64, marker: u64) -> u64 {
    SERVICE_MARKER_TAG
        | ((kind & 0xff) << SERVICE_MARKER_KIND_SHIFT)
        | (marker & SERVICE_MARKER_MASK)
}

/// SPEC-106：服务管理器向基础服务发**请求**的消息 tag（与 `SERVICE_WAKE_TAG`(0x5ECA) 不相交）。
///
/// SPEC-110：请求 payload = `tag(48..63) | op(40..47) | arg(0..31)`。[`SERVICE_SERVE_OP_ADVANCE`] 用
/// `arg` 推进服务的内部状态，[`SERVICE_SERVE_OP_QUERY`] 只读回当前状态（不改）。**定序**多条
/// （推进、推进、查询）即判决服务**有状态**：纯回声端点会回显 `arg`，无法产生查询返回的状态值。
pub const SERVICE_SERVE_TAG: u64 = 0x5EC9_0000_0000_0000;
/// SPEC-110：请求 payload 里 `op` 字段（操作语义）的左移位数。
pub const SERVICE_SERVE_OP_SHIFT: u64 = 40;
/// SPEC-110：`op=0` 推进状态（服务按自身语义把 `arg` 并入内部状态）。
pub const SERVICE_SERVE_OP_ADVANCE: u64 = 0;
/// SPEC-110：`op=1` 只读回当前状态（不改）。
pub const SERVICE_SERVE_OP_QUERY: u64 = 1;
/// SPEC-110：定序请求的两个推进入参（5 → 3）。三个基础服务的状态转移语义**各异**（日志累加 /
/// 存储移位追加 / Namespace 滚动指纹），故两次推进后查询返回的**三个值互异**——判决「请求经服务
/// 表路由到了各自的**有状态**服务二进制，而非任一无状态回声端点」。
pub const SERVICE_SERVE_ARG_A: u64 = 5;
pub const SERVICE_SERVE_ARG_B: u64 = 3;
/// SPEC-110：每个基础服务的定序请求条数（推进、推进、查询）。
pub const SERVICE_SERVE_PROBES: u64 = 3;
/// SPEC-110：把服务请求打包（`op` 为操作语义，`arg` 为低 32 位入参）。
pub const fn pack_service_request(op: u64, arg: u64) -> u64 {
    SERVICE_SERVE_TAG | ((op & 0xff) << SERVICE_SERVE_OP_SHIFT) | (arg & 0xffff_ffff)
}
/// SPEC-106：基础服务应答里 `kind` 字段的左移位数——应答 = `SERVICE_SERVE_TAG | kind<<32 | arg`。
/// 各服务应答的 `kind` 相异，构成「请求经服务表路由到了**正确的**服务二进制」的判决性字段。
pub const SERVICE_REPLY_KIND_SHIFT: u64 = 32;
/// SPEC-106/110：把服务应答打包（`kind` 为服务的行为码，`arg` 为低 32 位**当前状态**）。
pub const fn pack_service_reply(kind: u64, arg: u64) -> u64 {
    SERVICE_SERVE_TAG | ((kind & 0xff) << SERVICE_REPLY_KIND_SHIFT) | (arg & 0xffff_ffff)
}
/// SPEC-090：服务管理器可路由的行为码。`ALPHA`（默认，max 2048）与 `BETA`（max 1024）。
pub const SERVICE_BEHAVIOR_ALPHA: u64 = 1;
pub const SERVICE_BEHAVIOR_BETA: u64 = 2;
/// SPEC-106（S4-D3）：三个**基础服务**端点——日志、Namespace、基础存储。与 `ALPHA`/`BETA`
/// 共用同一服务表/注册表/创建路径，只是不参与 wire 协商，而是自报**二进制标记**作存在证据。
pub const SERVICE_BEHAVIOR_LOG: u64 = 3;
pub const SERVICE_BEHAVIOR_NS: u64 = 4;
pub const SERVICE_BEHAVIOR_STORE: u64 = 5;
/// SPEC-090：名字未注册时的解析结果（服务管理器据此拒绝转发）。
pub const SERVICE_BEHAVIOR_NONE: u64 = 0;
/// SPEC-088：`ALPHA` 行为的服务端 offer。`max_message_size` 取 2048，与客户端 4096 取交后
/// 得到 `max=2048`——「结果只可能来自服务端」的判决性字段。
pub const SERVICE_OFFER_ALPHA: ProtocolOffer = ProtocolOffer {
    major: 1,
    minor: 0,
    features: 0b101,
    required_features: 0b001,
    max_message_size: 2048,
};
/// SPEC-090：`BETA` 行为的服务端 offer。`max_message_size` 取 1024 而非 2048，使协商结果
/// （`max=1024`）**只可能来自被按名选中的 BETA 行为**，构成路由的判决性字段。
pub const SERVICE_OFFER_BETA: ProtocolOffer = ProtocolOffer {
    major: 1,
    minor: 0,
    features: 0b101,
    required_features: 0b001,
    max_message_size: 1024,
};

/// SPEC-093：服务管理器路由到的行为码 → 服务端 offer。`ALPHA` 用 [`SERVICE_OFFER_ALPHA`]、
/// `BETA` 用 [`SERVICE_OFFER_BETA`]，使协商结果**承载路由决策**。
pub const fn service_offer(behavior: u64) -> ProtocolOffer {
    if behavior == SERVICE_BEHAVIOR_BETA {
        SERVICE_OFFER_BETA
    } else {
        SERVICE_OFFER_ALPHA
    }
}

/// SPEC-088：把协商结果打包成一个字（`major<<48 | minor<<32 | max<<16 | features 低 16 位`）。
pub const fn pack_negotiated(negotiated: NegotiatedProtocol) -> u64 {
    ((negotiated.major as u64) << 48)
        | ((negotiated.minor as u64) << 32)
        | ((negotiated.max_message_size as u64) << 16)
        | (negotiated.features & 0xffff)
}

/// SPEC-088：解析对端 Hello 并用给定服务端 offer 协商；成功返回打包结果，失败返回 0。
pub fn negotiation_word(
    header_bytes: &[u8; MESSAGE_HEADER_LEN],
    offer_bytes: &[u8; OFFER_LEN],
    server_offer: ProtocolOffer,
) -> u64 {
    if MessageHeader::decode(*header_bytes).is_err() {
        return 0;
    }
    let Ok(offer) = ProtocolOffer::decode(*offer_bytes) else {
        return 0;
    };
    match negotiate(offer, server_offer) {
        Ok(negotiated) => pack_negotiated(negotiated),
        Err(_) => 0,
    }
}

/// SPEC-088：两个负例——`major` 不匹配的协商与魔数错误的解码；命中给显式码，否则给 `0xffff`
/// 哨兵（使「逻辑没跑」不可能伪装成成功）。
pub fn negotiation_negatives() -> u64 {
    let client = ProtocolOffer {
        major: 1,
        minor: 0,
        features: 1,
        required_features: 1,
        max_message_size: 4096,
    };
    let mismatched = ProtocolOffer {
        major: 2,
        ..SERVICE_OFFER_ALPHA
    };
    let major_mismatch = match negotiate(client, mismatched) {
        Err(NegotiationError::MajorMismatch) => 1u64,
        _ => 0xffff,
    };
    let bad_magic = match MessageHeader::decode([0u8; MESSAGE_HEADER_LEN]) {
        Err(CodecError::BadMagic) => 0u64,
        _ => 0xffff,
    };
    (major_mismatch << 32) | bad_magic
}

/// 单条服务消息的 wire header。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MessageHeader {
    pub major: u16,
    pub minor: u16,
    pub opcode: u16,
    pub flags: u16,
    pub request_id: u32,
    pub payload_len: u32,
}

impl MessageHeader {
    /// 将消息头编码到固定长度字节数组。
    pub const fn encode(self) -> [u8; MESSAGE_HEADER_LEN] {
        let mut bytes = [0u8; MESSAGE_HEADER_LEN];
        bytes[0] = MAGIC[0];
        bytes[1] = MAGIC[1];
        bytes[2] = MAGIC[2];
        bytes[3] = MAGIC[3];
        put_u16(&mut bytes, 4, self.major);
        put_u16(&mut bytes, 6, self.minor);
        put_u16(&mut bytes, 8, self.opcode);
        put_u16(&mut bytes, 10, self.flags);
        put_u32(&mut bytes, 12, self.request_id);
        put_u32(&mut bytes, 16, self.payload_len);
        bytes
    }

    /// 解码消息头，并拒绝错误魔数或零主版本。
    pub const fn decode(bytes: [u8; MESSAGE_HEADER_LEN]) -> Result<Self, CodecError> {
        if bytes[0] != MAGIC[0]
            || bytes[1] != MAGIC[1]
            || bytes[2] != MAGIC[2]
            || bytes[3] != MAGIC[3]
        {
            return Err(CodecError::BadMagic);
        }
        let major = get_u16(&bytes, 4);
        if major == 0 {
            return Err(CodecError::InvalidVersion);
        }
        Ok(Self {
            major,
            minor: get_u16(&bytes, 6),
            opcode: get_u16(&bytes, 8),
            flags: get_u16(&bytes, 10),
            request_id: get_u32(&bytes, 12),
            payload_len: get_u32(&bytes, 16),
        })
    }
}

/// 一端支持的协议主次版本和可选能力。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProtocolOffer {
    /// 不兼容变更时递增；必须非零且双方相同。
    pub major: u16,
    /// 同一主版本内向后兼容的扩展版本。
    pub minor: u16,
    /// 此端可用的 feature 位集。
    pub features: u64,
    /// 此端要求对端也支持的 feature 位集，必须是 `features` 子集。
    pub required_features: u64,
    /// 本端能处理的最大单条消息长度。
    pub max_message_size: u32,
}

impl ProtocolOffer {
    /// 按 wire format 编码为 little-endian 固定长度 payload。
    pub const fn encode(self) -> [u8; OFFER_LEN] {
        let mut bytes = [0u8; OFFER_LEN];
        put_u16(&mut bytes, 0, self.major);
        put_u16(&mut bytes, 2, self.minor);
        put_u64(&mut bytes, 4, self.features);
        put_u64(&mut bytes, 12, self.required_features);
        put_u32(&mut bytes, 20, self.max_message_size);
        bytes
    }

    /// 从固定长度 little-endian payload 解码服务协议 offer。
    pub const fn decode(bytes: [u8; OFFER_LEN]) -> Result<Self, CodecError> {
        let offer = Self {
            major: get_u16(&bytes, 0),
            minor: get_u16(&bytes, 2),
            features: get_u64(&bytes, 4),
            required_features: get_u64(&bytes, 12),
            max_message_size: get_u32(&bytes, 20),
        };
        if offer.major == 0
            || offer.max_message_size == 0
            || offer.required_features & !offer.features != 0
        {
            Err(CodecError::InvalidOffer)
        } else {
            Ok(offer)
        }
    }
}

/// Wire 编码错误。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CodecError {
    BadMagic,
    InvalidVersion,
    InvalidOffer,
}

const fn put_u16<const N: usize>(bytes: &mut [u8; N], offset: usize, value: u16) {
    let encoded = value.to_le_bytes();
    bytes[offset] = encoded[0];
    bytes[offset + 1] = encoded[1];
}

const fn put_u32<const N: usize>(bytes: &mut [u8; N], offset: usize, value: u32) {
    let encoded = value.to_le_bytes();
    bytes[offset] = encoded[0];
    bytes[offset + 1] = encoded[1];
    bytes[offset + 2] = encoded[2];
    bytes[offset + 3] = encoded[3];
}

const fn put_u64<const N: usize>(bytes: &mut [u8; N], offset: usize, value: u64) {
    let encoded = value.to_le_bytes();
    let mut index = 0;
    while index < encoded.len() {
        bytes[offset + index] = encoded[index];
        index += 1;
    }
}

const fn get_u16<const N: usize>(bytes: &[u8; N], offset: usize) -> u16 {
    u16::from_le_bytes([bytes[offset], bytes[offset + 1]])
}

const fn get_u32<const N: usize>(bytes: &[u8; N], offset: usize) -> u32 {
    u32::from_le_bytes([
        bytes[offset],
        bytes[offset + 1],
        bytes[offset + 2],
        bytes[offset + 3],
    ])
}

const fn get_u64<const N: usize>(bytes: &[u8; N], offset: usize) -> u64 {
    u64::from_le_bytes([
        bytes[offset],
        bytes[offset + 1],
        bytes[offset + 2],
        bytes[offset + 3],
        bytes[offset + 4],
        bytes[offset + 5],
        bytes[offset + 6],
        bytes[offset + 7],
    ])
}

/// 协商成功后的共同协议参数。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NegotiatedProtocol {
    pub major: u16,
    /// 取双方支持次版本的较小值。
    pub minor: u16,
    /// 双方 feature 位集的交集。
    pub features: u64,
    /// 双方消息尺寸上限的较小值。
    pub max_message_size: u32,
}

/// 协商失败的原因。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NegotiationError {
    InvalidOffer,
    MajorMismatch,
    RequiredFeatureUnavailable,
}

/// 按固定规则协商服务协议。
///
/// 两端主版本必须相同且非零；共同次版本取较小值；必需 feature 必须在双方的 feature
/// 交集中；消息尺寸上限取较小值。调用方应在发送任何有副作用的服务请求前完成协商。
pub const fn negotiate(
    client: ProtocolOffer,
    server: ProtocolOffer,
) -> Result<NegotiatedProtocol, NegotiationError> {
    if client.major == 0
        || server.major == 0
        || client.max_message_size == 0
        || server.max_message_size == 0
        || client.required_features & !client.features != 0
        || server.required_features & !server.features != 0
    {
        return Err(NegotiationError::InvalidOffer);
    }
    if client.major != server.major {
        return Err(NegotiationError::MajorMismatch);
    }

    let features = client.features & server.features;
    if (client.required_features | server.required_features) & !features != 0 {
        return Err(NegotiationError::RequiredFeatureUnavailable);
    }

    Ok(NegotiatedProtocol {
        major: client.major,
        minor: if client.minor < server.minor {
            client.minor
        } else {
            server.minor
        },
        features,
        max_message_size: if client.max_message_size < server.max_message_size {
            client.max_message_size
        } else {
            server.max_message_size
        },
    })
}
