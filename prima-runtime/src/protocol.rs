//! 服务 wire protocol 的版本和 feature 协商。

/// 消息头固定长度。各字段显式按 little-endian 编码，不依赖 Rust 结构体布局。
pub const MESSAGE_HEADER_LEN: usize = 20;
/// Hello offer 固定长度。
pub const OFFER_LEN: usize = 24;
const MAGIC: [u8; 4] = *b"PRIM";

/// SPEC-090：客户端在共享段内写入**请求服务名**的偏移（位于 20 B 头 + 24 B offer 之后）。
/// 内核侧 init（服务管理器）从同一物理帧的映射读回该名字以解析端点。
pub const SERVICE_NAME_OFFSET: u64 = 48;
/// SPEC-090：注册的服务名。`BETA` 被服务管理器解析为行为码 2；未注册名解析为 0（拒绝）。
pub const SERVICE_NAME_BETA: u64 = 2;
/// SPEC-090：服务管理器可路由的行为码。`ALPHA`（默认，max 2048）与 `BETA`（max 1024）。
pub const SERVICE_BEHAVIOR_ALPHA: u64 = 1;
pub const SERVICE_BEHAVIOR_BETA: u64 = 2;
/// SPEC-090：名字未注册时的解析结果（服务管理器据此拒绝转发）。
pub const SERVICE_BEHAVIOR_NONE: u64 = 0;
/// SPEC-090：`BETA` 行为的服务端 offer。`max_message_size` 取 1024 而非 2048，使协商结果
/// （`max=1024`）**只可能来自被按名选中的 BETA 行为**，构成路由的判决性字段。
pub const SERVICE_OFFER_BETA: ProtocolOffer = ProtocolOffer {
    major: 1,
    minor: 0,
    features: 0b101,
    required_features: 0b001,
    max_message_size: 1024,
};

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
