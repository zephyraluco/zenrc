# CycloneDDS Bindings API 分类文档

> 由 rust-bindgen 0.72.1 自动生成，本文档对 `bindings.rs` 中的全部 326 个外部函数按功能分类整理。

---

## 目录

- [CycloneDDS Bindings API 分类文档](#cyclonedds-bindings-api-分类文档)
  - [目录](#目录)
  - [1. 时间与工具](#1-时间与工具)
  - [2. 日志与诊断](#2-日志与诊断)
  - [3. 内存管理](#3-内存管理)
  - [4. QoS 管理](#4-qos-管理)
    - [4.1 QoS 对象生命周期](#41-qos-对象生命周期)
    - [4.2 QoS 属性设置 (qset)](#42-qos-属性设置-qset)
    - [4.3 QoS 属性读取 (qget)](#43-qos-属性读取-qget)
    - [4.4 QoS 提供者](#44-qos-提供者)
  - [5. 错误处理](#5-错误处理)
  - [6. 实体管理](#6-实体管理)
  - [7. 状态查询](#7-状态查询)
  - [8. 监听器管理](#8-监听器管理)
    - [8.1 监听器生命周期](#81-监听器生命周期)
    - [8.2 设置回调（带 arg）](#82-设置回调带-arg)
    - [8.3 设置回调（不带 arg）](#83-设置回调不带-arg)
    - [8.4 读取回调（带 arg）](#84-读取回调带-arg)
    - [8.5 读取回调（不带 arg）](#85-读取回调不带-arg)
  - [9. 参与者与域](#9-参与者与域)
  - [10. Topic 管理](#10-topic-管理)
  - [11. 发布者与订阅者](#11-发布者与订阅者)
  - [12. 读者与写者](#12-读者与写者)
  - [13. 数据写入](#13-数据写入)
  - [14. 数据读取](#14-数据读取)
    - [14.1 Peek 操作](#141-peek-操作)
    - [14.2 Read 操作](#142-read-操作)
    - [14.3 Take 操作](#143-take-操作)
    - [14.4 CDR 序列化读取](#144-cdr-序列化读取)
    - [14.5 带 Collector 的读取](#145-带-collector-的读取)
  - [15. 条件与等待集](#15-条件与等待集)
  - [16. 实例管理](#16-实例管理)
  - [17. 内置 Topic 与匹配信息](#17-内置-topic-与匹配信息)
  - [18. 借用与共享内存](#18-借用与共享内存)
  - [19. 动态类型](#19-动态类型)
    - [类型构建](#类型构建)
    - [成员属性](#成员属性)
    - [类型引用管理](#类型引用管理)
  - [20. 类型信息](#20-类型信息)

---

## 1. 时间与工具

| 函数 | 签名 | 说明 |
|------|------|------|
| `dds_time` | `() -> dds_time_t` | 获取当前 DDS 时间（纳秒） |
| `dds_sleepfor` | `(reltime: dds_duration_t)` | 休眠指定时长 |

---

## 2. 日志与诊断

| 函数 | 签名 | 说明 |
|------|------|------|
| `dds_set_log_mask` | `(cats: u32)` | 设置日志类别掩码 |
| `dds_set_log_file` | `(file: *mut FILE)` | 设置日志输出文件 |
| `dds_set_trace_file` | `(file: *mut FILE)` | 设置跟踪输出文件 |
| `dds_set_log_sink` | `(callback: dds_log_write_fn_t, userdata: *mut c_void)` | 设置日志回调 |
| `dds_set_trace_sink` | `(callback: dds_log_write_fn_t, userdata: *mut c_void)` | 设置跟踪回调 |
| `dds_log_cfg_init` | `(cfg: *mut ddsrt_log_cfg, domid: u32, tracemask: u32, log_fp: *mut FILE, trace_fp: *mut FILE)` | 初始化日志配置 |
| `dds_log_cfg` | `(cfg: *const ddsrt_log_cfg, cat: u32, file: *const c_char, line: u32, func: *const c_char, fmt: *const c_char, ...)` | 通过配置写日志 |
| `dds_log_id` | `(cat: u32, domid: u32, file: *const c_char, line: u32, func: *const c_char, fmt: *const c_char, ...)` | 写带域 ID 的日志 |
| `dds_log` | `(cat: u32, file: *const c_char, line: u32, func: *const c_char, fmt: *const c_char, ...)` | 写日志 |

---

## 3. 内存管理

| 函数 | 签名 | 说明 |
|------|------|------|
| `dds_alloc` | `(size: usize) -> *mut c_void` | 分配内存 |
| `dds_realloc` | `(ptr: *mut c_void, size: usize) -> *mut c_void` | 重新分配内存 |
| `dds_realloc_zero` | `(ptr: *mut c_void, size: usize) -> *mut c_void` | 重新分配内存并清零 |
| `dds_free` | `(ptr: *mut c_void)` | 释放内存 |
| `dds_string_alloc` | `(size: usize) -> *mut c_char` | 分配字符串缓冲区 |
| `dds_string_dup` | `(str_: *const c_char) -> *mut c_char` | 复制字符串 |
| `dds_string_free` | `(str_: *mut c_char)` | 释放字符串 |
| `dds_sample_free` | `(sample: *mut c_void, desc: *const dds_topic_descriptor, op: dds_free_op_t)` | 释放样本数据 |

---

## 4. QoS 管理

### 4.1 QoS 对象生命周期

| 函数 | 签名 | 说明 |
|------|------|------|
| `dds_create_qos` | `() -> *mut dds_qos_t` | 创建 QoS 对象 |
| `dds_delete_qos` | `(qos: *mut dds_qos_t)` | 销毁 QoS 对象 |
| `dds_reset_qos` | `(qos: *mut dds_qos_t)` | 重置 QoS 为默认值 |
| `dds_copy_qos` | `(dst: *mut dds_qos_t, src: *const dds_qos_t) -> dds_return_t` | 复制 QoS |
| `dds_merge_qos` | `(dst: *mut dds_qos_t, src: *const dds_qos_t)` | 合并 QoS |
| `dds_qos_equal` | `(a: *const dds_qos_t, b: *const dds_qos_t) -> bool` | 比较两个 QoS 是否相等 |

### 4.2 QoS 属性设置 (qset)

| 函数 | 说明 |
|------|------|
| `dds_qset_userdata` | 设置用户数据 |
| `dds_qset_topicdata` | 设置 Topic 数据 |
| `dds_qset_groupdata` | 设置组数据 |
| `dds_qset_durability` | 设置持久性策略 |
| `dds_qset_history` | 设置历史策略 |
| `dds_qset_resource_limits` | 设置资源限制 |
| `dds_qset_presentation` | 设置呈现策略 |
| `dds_qset_lifespan` | 设置数据生命周期 |
| `dds_qset_deadline` | 设置截止时间 |
| `dds_qset_latency_budget` | 设置延迟预算 |
| `dds_qset_ownership` | 设置所有权策略 |
| `dds_qset_ownership_strength` | 设置所有权强度 |
| `dds_qset_liveliness` | 设置活跃性策略 |
| `dds_qset_time_based_filter` | 设置基于时间的过滤器 |
| `dds_qset_partition` | 设置分区（多个） |
| `dds_qset_partition1` | 设置单个分区 |
| `dds_qset_reliability` | 设置可靠性策略 |
| `dds_qset_transport_priority` | 设置传输优先级 |
| `dds_qset_destination_order` | 设置目标排序策略 |
| `dds_qset_writer_data_lifecycle` | 设置写者数据生命周期 |
| `dds_qset_reader_data_lifecycle` | 设置读者数据生命周期 |
| `dds_qset_writer_batching` | 设置写者批量模式 |
| `dds_qset_durability_service` | 设置持久性服务策略 |
| `dds_qset_ignorelocal` | 设置忽略本地策略 |
| `dds_qset_prop` | 设置字符串属性 |
| `dds_qset_prop_propagate` | 设置可传播字符串属性 |
| `dds_qunset_prop` | 删除字符串属性 |
| `dds_qset_bprop` | 设置二进制属性 |
| `dds_qset_bprop_propagate` | 设置可传播二进制属性 |
| `dds_qunset_bprop` | 删除二进制属性 |
| `dds_qset_type_consistency` | 设置类型一致性策略 |
| `dds_qset_data_representation` | 设置数据表示方式 |
| `dds_qset_entity_name` | 设置实体名称 |
| `dds_qset_psmx_instances` | 设置 PSMX 实例列表 |

### 4.3 QoS 属性读取 (qget)

| 函数 | 说明 |
|------|------|
| `dds_qget_userdata` | 读取用户数据 |
| `dds_qget_topicdata` | 读取 Topic 数据 |
| `dds_qget_groupdata` | 读取组数据 |
| `dds_qget_durability` | 读取持久性策略 |
| `dds_qget_history` | 读取历史策略 |
| `dds_qget_resource_limits` | 读取资源限制 |
| `dds_qget_presentation` | 读取呈现策略 |
| `dds_qget_lifespan` | 读取数据生命周期 |
| `dds_qget_deadline` | 读取截止时间 |
| `dds_qget_latency_budget` | 读取延迟预算 |
| `dds_qget_ownership` | 读取所有权策略 |
| `dds_qget_ownership_strength` | 读取所有权强度 |
| `dds_qget_liveliness` | 读取活跃性策略 |
| `dds_qget_time_based_filter` | 读取基于时间的过滤器 |
| `dds_qget_partition` | 读取分区列表 |
| `dds_qget_reliability` | 读取可靠性策略 |
| `dds_qget_transport_priority` | 读取传输优先级 |
| `dds_qget_destination_order` | 读取目标排序策略 |
| `dds_qget_writer_data_lifecycle` | 读取写者数据生命周期 |
| `dds_qget_reader_data_lifecycle` | 读取读者数据生命周期 |
| `dds_qget_writer_batching` | 读取写者批量模式 |
| `dds_qget_durability_service` | 读取持久性服务策略 |
| `dds_qget_ignorelocal` | 读取忽略本地策略 |
| `dds_qget_propnames` | 读取所有字符串属性名 |
| `dds_qget_prop` | 读取字符串属性值 |
| `dds_qget_prop_propagate` | 读取可传播字符串属性 |
| `dds_qget_bpropnames` | 读取所有二进制属性名 |
| `dds_qget_bprop` | 读取二进制属性值 |
| `dds_qget_bprop_propagate` | 读取可传播二进制属性 |
| `dds_qget_type_consistency` | 读取类型一致性策略 |
| `dds_qget_data_representation` | 读取数据表示方式 |
| `dds_qget_entity_name` | 读取实体名称 |
| `dds_qget_psmx_instances` | 读取 PSMX 实例列表 |

### 4.4 QoS 提供者

| 函数 | 签名 | 说明 |
|------|------|------|
| `dds_create_qos_provider` | `(xml: *const c_char, provider: *mut *mut dds_qos_provider_t) -> dds_return_t` | 从 XML 创建 QoS 提供者 |
| `dds_create_qos_provider_scope` | `(xml: *const c_char, scope: *const c_char, provider: *mut *mut dds_qos_provider_t) -> dds_return_t` | 从 XML+作用域创建 QoS 提供者 |
| `dds_qos_provider_get_qos` | `(provider: *const dds_qos_provider_t, entity_kind: u32, id: *const c_char, qos: *mut *mut dds_qos_t) -> dds_return_t` | 从提供者获取 QoS |
| `dds_delete_qos_provider` | `(provider: *mut dds_qos_provider_t)` | 销毁 QoS 提供者 |

---

## 5. 错误处理

| 函数 | 签名 | 说明 |
|------|------|------|
| `dds_strretcode` | `(ret: dds_return_t) -> *const c_char` | 返回码转字符串 |
| `dds_err_str` | `(err: dds_return_t) -> *const c_char` | 错误码转字符串 |
| `dds_fail` | `(msg: *const c_char, where_: *const c_char)` | 记录失败信息 |
| `dds_fail_set` | `(fn_: dds_fail_fn)` | 设置失败回调 |
| `dds_fail_get` | `() -> dds_fail_fn` | 获取当前失败回调 |
| `dds_err_check` | `(err: dds_return_t, flags: u32, where_: *const c_char) -> dds_return_t` | 检查并处理错误 |

---

## 6. 实体管理

| 函数 | 签名 | 说明 |
|------|------|------|
| `dds_enable` | `(entity: dds_entity_t) -> dds_return_t` | 启用实体 |
| `dds_delete` | `(entity: dds_entity_t) -> dds_return_t` | 删除实体 |
| `dds_get_publisher` | `(writer: dds_entity_t) -> dds_entity_t` | 获取写者的发布者 |
| `dds_get_subscriber` | `(entity: dds_entity_t) -> dds_entity_t` | 获取读者的订阅者 |
| `dds_get_datareader` | `(entity: dds_entity_t) -> dds_entity_t` | 获取关联读者 |
| `dds_get_mask` | `(entity: dds_entity_t, mask: *mut u32) -> dds_return_t` | 获取实体掩码 |
| `dds_get_instance_handle` | `(entity: dds_entity_t, ihdl: *mut dds_instance_handle_t) -> dds_return_t` | 获取实例句柄 |
| `dds_get_guid` | `(entity: dds_entity_t, guid: *mut dds_guid_t) -> dds_return_t` | 获取实体 GUID |
| `dds_read_status` | `(entity: dds_entity_t, status: *mut u32, mask: u32) -> dds_return_t` | 读取并重置状态 |
| `dds_take_status` | `(entity: dds_entity_t, status: *mut u32, mask: u32) -> dds_return_t` | 获取并清除状态 |
| `dds_get_status_changes` | `(entity: dds_entity_t, status: *mut u32) -> dds_return_t` | 获取所有未读状态变化 |
| `dds_get_status_mask` | `(entity: dds_entity_t, mask: *mut u32) -> dds_return_t` | 获取状态掩码 |
| `dds_set_status_mask` | `(entity: dds_entity_t, mask: u32) -> dds_return_t` | 设置状态掩码 |
| `dds_get_qos` | `(entity: dds_entity_t, qos: *mut dds_qos_t) -> dds_return_t` | 获取实体 QoS |
| `dds_set_qos` | `(entity: dds_entity_t, qos: *const dds_qos_t) -> dds_return_t` | 设置实体 QoS |
| `dds_get_listener` | `(entity: dds_entity_t, listener: *mut dds_listener_t) -> dds_return_t` | 获取实体监听器 |
| `dds_set_listener` | `(entity: dds_entity_t, listener: *const dds_listener_t) -> dds_return_t` | 设置实体监听器 |
| `dds_get_parent` | `(entity: dds_entity_t) -> dds_entity_t` | 获取父实体 |
| `dds_get_participant` | `(entity: dds_entity_t) -> dds_entity_t` | 获取所属参与者 |
| `dds_get_children` | `(entity: dds_entity_t, children: *mut dds_entity_t, size: usize) -> dds_return_t` | 获取子实体列表 |
| `dds_get_domainid` | `(entity: dds_entity_t, id: *mut dds_domainid_t) -> dds_return_t` | 获取域 ID |

---

## 7. 状态查询

| 函数 | 说明 |
|------|------|
| `dds_get_inconsistent_topic_status` | 读取 Topic 不一致状态 |
| `dds_get_publication_matched_status` | 读取发布匹配状态 |
| `dds_get_liveliness_lost_status` | 读取活跃性丢失状态 |
| `dds_get_offered_deadline_missed_status` | 读取发布方截止时间错过状态 |
| `dds_get_offered_incompatible_qos_status` | 读取发布方不兼容 QoS 状态 |
| `dds_get_subscription_matched_status` | 读取订阅匹配状态 |
| `dds_get_liveliness_changed_status` | 读取活跃性变化状态 |
| `dds_get_sample_rejected_status` | 读取样本拒绝状态 |
| `dds_get_sample_lost_status` | 读取样本丢失状态 |
| `dds_get_requested_deadline_missed_status` | 读取订阅方截止时间错过状态 |
| `dds_get_requested_incompatible_qos_status` | 读取订阅方不兼容 QoS 状态 |

---

## 8. 监听器管理

### 8.1 监听器生命周期

| 函数 | 签名 | 说明 |
|------|------|------|
| `dds_create_listener` | `(arg: *mut c_void) -> *mut dds_listener_t` | 创建监听器 |
| `dds_delete_listener` | `(listener: *mut dds_listener_t)` | 销毁监听器 |
| `dds_reset_listener` | `(listener: *mut dds_listener_t)` | 重置所有回调为 NULL |
| `dds_copy_listener` | `(dst: *mut dds_listener_t, src: *const dds_listener_t)` | 复制监听器 |
| `dds_merge_listener` | `(dst: *mut dds_listener_t, src: *const dds_listener_t)` | 合并监听器 |

### 8.2 设置回调（带 arg）

以下函数均以 `dds_lset_<事件>_arg` 命名，参数包含 `listener`、`callback`、`arg`、`reset_on_invoke`：

| 函数 | 对应事件 |
|------|---------|
| `dds_lset_data_available_arg` | 数据可用 |
| `dds_lset_data_on_readers_arg` | 读者上有数据 |
| `dds_lset_inconsistent_topic_arg` | Topic 不一致 |
| `dds_lset_liveliness_changed_arg` | 活跃性变化 |
| `dds_lset_liveliness_lost_arg` | 活跃性丢失 |
| `dds_lset_offered_deadline_missed_arg` | 发布截止时间错过 |
| `dds_lset_offered_incompatible_qos_arg` | 发布 QoS 不兼容 |
| `dds_lset_publication_matched_arg` | 发布匹配 |
| `dds_lset_requested_deadline_missed_arg` | 订阅截止时间错过 |
| `dds_lset_requested_incompatible_qos_arg` | 订阅 QoS 不兼容 |
| `dds_lset_sample_lost_arg` | 样本丢失 |
| `dds_lset_sample_rejected_arg` | 样本拒绝 |
| `dds_lset_subscription_matched_arg` | 订阅匹配 |

### 8.3 设置回调（不带 arg）

| 函数 | 对应事件 |
|------|---------|
| `dds_lset_inconsistent_topic` | Topic 不一致 |
| `dds_lset_liveliness_lost` | 活跃性丢失 |
| `dds_lset_offered_deadline_missed` | 发布截止时间错过 |
| `dds_lset_offered_incompatible_qos` | 发布 QoS 不兼容 |
| `dds_lset_data_on_readers` | 读者上有数据 |
| `dds_lset_sample_lost` | 样本丢失 |
| `dds_lset_data_available` | 数据可用 |
| `dds_lset_sample_rejected` | 样本拒绝 |
| `dds_lset_liveliness_changed` | 活跃性变化 |
| `dds_lset_requested_deadline_missed` | 订阅截止时间错过 |
| `dds_lset_requested_incompatible_qos` | 订阅 QoS 不兼容 |
| `dds_lset_publication_matched` | 发布匹配 |
| `dds_lset_subscription_matched` | 订阅匹配 |

### 8.4 读取回调（带 arg）

| 函数 | 对应事件 |
|------|---------|
| `dds_lget_data_available_arg` | 数据可用 |
| `dds_lget_data_on_readers_arg` | 读者上有数据 |
| `dds_lget_inconsistent_topic_arg` | Topic 不一致 |
| `dds_lget_liveliness_changed_arg` | 活跃性变化 |
| `dds_lget_liveliness_lost_arg` | 活跃性丢失 |
| `dds_lget_offered_deadline_missed_arg` | 发布截止时间错过 |
| `dds_lget_offered_incompatible_qos_arg` | 发布 QoS 不兼容 |
| `dds_lget_publication_matched_arg` | 发布匹配 |
| `dds_lget_requested_deadline_missed_arg` | 订阅截止时间错过 |
| `dds_lget_requested_incompatible_qos_arg` | 订阅 QoS 不兼容 |
| `dds_lget_sample_lost_arg` | 样本丢失 |
| `dds_lget_sample_rejected_arg` | 样本拒绝 |
| `dds_lget_subscription_matched_arg` | 订阅匹配 |

### 8.5 读取回调（不带 arg）

| 函数 | 对应事件 |
|------|---------|
| `dds_lget_inconsistent_topic` | Topic 不一致 |
| `dds_lget_liveliness_lost` | 活跃性丢失 |
| `dds_lget_offered_deadline_missed` | 发布截止时间错过 |
| `dds_lget_offered_incompatible_qos` | 发布 QoS 不兼容 |
| `dds_lget_data_on_readers` | 读者上有数据 |
| `dds_lget_sample_lost` | 样本丢失 |
| `dds_lget_data_available` | 数据可用 |
| `dds_lget_sample_rejected` | 样本拒绝 |
| `dds_lget_liveliness_changed` | 活跃性变化 |
| `dds_lget_requested_deadline_missed` | 订阅截止时间错过 |
| `dds_lget_requested_incompatible_qos` | 订阅 QoS 不兼容 |
| `dds_lget_publication_matched` | 发布匹配 |
| `dds_lget_subscription_matched` | 订阅匹配 |

---

## 9. 参与者与域

| 函数 | 签名 | 说明 |
|------|------|------|
| `dds_create_participant` | `(domain: dds_domainid_t, qos: *const dds_qos_t, listener: *const dds_listener_t) -> dds_entity_t` | 创建参与者 |
| `dds_create_domain` | `(domain: dds_domainid_t, config: *const c_char) -> dds_entity_t` | 创建域（XML 配置） |
| `dds_create_domain_with_rawconfig` | `(domain: dds_domainid_t, config: *const ddsi_config) -> dds_entity_t` | 创建域（结构体配置） |
| `dds_lookup_participant` | `(domain: dds_domainid_t, participants: *mut dds_entity_t, size: usize) -> dds_return_t` | 查找域内参与者 |

---

## 10. Topic 管理

| 函数 | 签名 | 说明 |
|------|------|------|
| `dds_create_topic` | `(participant: dds_entity_t, descriptor: *const dds_topic_descriptor_t, name: *const c_char, qos: *const dds_qos_t, listener: *const dds_listener_t) -> dds_entity_t` | 创建 Topic |
| `dds_create_topic_sertype` | `(participant: dds_entity_t, name: *const c_char, sertype: *mut *mut ddsi_sertype, qos: *const dds_qos_t, listener: *const dds_listener_t, sdesc: *const ddsi_serdata) -> dds_entity_t` | 使用序列化类型创建 Topic |
| `dds_find_topic` | `(scope: dds_find_scope_t, participant: dds_entity_t, name: *const c_char, type_info: *const dds_typeinfo_t, timeout: dds_duration_t) -> dds_entity_t` | 查找 Topic |
| `dds_find_topic_scoped` | `(scope: dds_find_scope_t, participant: dds_entity_t, name: *const c_char, timeout: dds_duration_t) -> dds_entity_t` | 按作用域查找 Topic |
| `dds_create_topic_descriptor` | `(kind: dds_topic_descriptor_kind_t, sertype: *mut ddsi_sertype, ...) -> dds_return_t` | 创建 Topic 描述符 |
| `dds_delete_topic_descriptor` | `(descriptor: *mut dds_topic_descriptor_t)` | 删除 Topic 描述符 |
| `dds_get_name` | `(topic: dds_entity_t, name: *mut c_char, size: usize) -> dds_return_t` | 获取 Topic 名称 |
| `dds_get_type_name` | `(topic: dds_entity_t, name: *mut c_char, size: usize) -> dds_return_t` | 获取 Topic 类型名称 |
| `dds_set_topic_filter_and_arg` | `(topic: dds_entity_t, filter: dds_topic_filter_and_arg_fn, arg: *mut c_void)` | 设置带参数的 Topic 过滤函数 |
| `dds_set_topic_filter_extended` | `(topic: dds_entity_t, filter: *const dds_topic_filter_extended_t)` | 设置扩展 Topic 过滤器 |
| `dds_get_topic_filter_and_arg` | `(topic: dds_entity_t, fn_: *mut dds_topic_filter_and_arg_fn, arg: *mut *mut c_void)` | 读取带参数的 Topic 过滤函数 |
| `dds_get_topic_filter_extended` | `(topic: dds_entity_t, filter: *mut dds_topic_filter_extended_t)` | 读取扩展 Topic 过滤器 |

---

## 11. 发布者与订阅者

| 函数 | 签名 | 说明 |
|------|------|------|
| `dds_create_publisher` | `(participant: dds_entity_t, qos: *const dds_qos_t, listener: *const dds_listener_t) -> dds_entity_t` | 创建发布者 |
| `dds_create_subscriber` | `(participant: dds_entity_t, qos: *const dds_qos_t, listener: *const dds_listener_t) -> dds_entity_t` | 创建订阅者 |
| `dds_suspend` | `(publisher: dds_entity_t) -> dds_return_t` | 挂起发布者（批量写入） |
| `dds_resume` | `(publisher: dds_entity_t) -> dds_return_t` | 恢复发布者 |
| `dds_wait_for_acks` | `(publisher: dds_entity_t, timeout: dds_duration_t) -> dds_return_t` | 等待所有写者的确认 |

---

## 12. 读者与写者

| 函数 | 签名 | 说明 |
|------|------|------|
| `dds_create_writer` | `(participant_or_publisher: dds_entity_t, topic: dds_entity_t, qos: *const dds_qos_t, listener: *const dds_listener_t) -> dds_entity_t` | 创建写者 |
| `dds_create_reader` | `(participant_or_subscriber: dds_entity_t, topic: dds_entity_t, qos: *const dds_qos_t, listener: *const dds_listener_t) -> dds_entity_t` | 创建读者 |
| `dds_create_reader_rhc` | `(participant_or_subscriber: dds_entity_t, topic: dds_entity_t, qos: *const dds_qos_t, listener: *const dds_listener_t, rhc: *mut dds_reader_history_cache_t) -> dds_entity_t` | 创建带自定义历史缓存的读者 |
| `dds_reader_wait_for_historical_data` | `(reader: dds_entity_t, max_wait: dds_duration_t) -> dds_return_t` | 等待历史数据到达 |

---

## 13. 数据写入

| 函数 | 签名 | 说明 |
|------|------|------|
| `dds_write_set_batch` | `(enable: bool)` | 全局开启/关闭批量写入 |
| `dds_register_instance` | `(writer: dds_entity_t, data: *const c_void, handle: *mut dds_instance_handle_t) -> dds_return_t` | 注册实例 |
| `dds_unregister_instance` | `(writer: dds_entity_t, data: *const c_void) -> dds_return_t` | 注销实例（按数据键值） |
| `dds_unregister_instance_ih` | `(writer: dds_entity_t, handle: dds_instance_handle_t) -> dds_return_t` | 注销实例（按句柄） |
| `dds_unregister_instance_ts` | `(writer: dds_entity_t, data: *const c_void, timestamp: dds_time_t) -> dds_return_t` | 注销实例（带时间戳） |
| `dds_unregister_instance_ih_ts` | `(writer: dds_entity_t, handle: dds_instance_handle_t, timestamp: dds_time_t) -> dds_return_t` | 注销实例（句柄+时间戳） |
| `dds_write` | `(writer: dds_entity_t, data: *const c_void) -> dds_return_t` | 写入数据 |
| `dds_write_flush` | `(entity: dds_entity_t) -> dds_return_t` | 刷新缓冲区 |
| `dds_writecdr` | `(writer: dds_entity_t, serdata: *mut ddsi_serdata) -> dds_return_t` | 写入 CDR 序列化数据 |
| `dds_forwardcdr` | `(writer: dds_entity_t, serdata: *mut ddsi_serdata) -> dds_return_t` | 转发 CDR 序列化数据 |
| `dds_write_ts` | `(writer: dds_entity_t, data: *const c_void, timestamp: dds_time_t) -> dds_return_t` | 带时间戳写入数据 |
| `dds_writedispose` | `(writer: dds_entity_t, data: *const c_void) -> dds_return_t` | 写入并处置数据 |
| `dds_writedispose_ts` | `(writer: dds_entity_t, data: *const c_void, timestamp: dds_time_t) -> dds_return_t` | 带时间戳写入并处置 |
| `dds_dispose` | `(writer: dds_entity_t, data: *const c_void) -> dds_return_t` | 处置实例（按数据键值） |
| `dds_dispose_ts` | `(writer: dds_entity_t, data: *const c_void, timestamp: dds_time_t) -> dds_return_t` | 处置实例（带时间戳） |
| `dds_dispose_ih` | `(writer: dds_entity_t, handle: dds_instance_handle_t) -> dds_return_t` | 处置实例（按句柄） |
| `dds_dispose_ih_ts` | `(writer: dds_entity_t, handle: dds_instance_handle_t, timestamp: dds_time_t) -> dds_return_t` | 处置实例（句柄+时间戳） |

---

## 14. 数据读取

### 14.1 Peek 操作

Peek 读取数据但**不修改**样本/实例状态。

| 函数 | 说明 |
|------|------|
| `dds_peek` | 读取数据（不改状态） |
| `dds_peek_mask` | 带掩码读取数据（不改状态） |
| `dds_peek_instance` | 读取指定实例数据（不改状态） |
| `dds_peek_instance_mask` | 带掩码读取指定实例（不改状态） |
| `dds_peek_next` | 读取下一条数据（不改状态） |

### 14.2 Read 操作

Read 读取数据，将样本标记为已读（**不移除**）。

| 函数 | 说明 |
|------|------|
| `dds_read` | 读取数据 |
| `dds_read_wl` | 读取数据（借用内存） |
| `dds_read_mask` | 带掩码读取数据 |
| `dds_read_mask_wl` | 带掩码读取数据（借用内存） |
| `dds_read_instance` | 读取指定实例数据 |
| `dds_read_instance_wl` | 读取指定实例数据（借用内存） |
| `dds_read_instance_mask` | 带掩码读取指定实例数据 |
| `dds_read_instance_mask_wl` | 带掩码读取指定实例（借用内存） |
| `dds_read_next` | 读取下一条未读数据 |
| `dds_read_next_wl` | 读取下一条未读数据（借用内存） |

### 14.3 Take 操作

Take 读取数据并**移除**出读者缓存。

| 函数 | 说明 |
|------|------|
| `dds_take` | 取出数据 |
| `dds_take_wl` | 取出数据（借用内存） |
| `dds_take_mask` | 带掩码取出数据 |
| `dds_take_mask_wl` | 带掩码取出数据（借用内存） |
| `dds_take_instance` | 取出指定实例数据 |
| `dds_take_instance_wl` | 取出指定实例数据（借用内存） |
| `dds_take_instance_mask` | 带掩码取出指定实例数据 |
| `dds_take_instance_mask_wl` | 带掩码取出指定实例（借用内存） |
| `dds_take_next` | 取出下一条数据 |
| `dds_take_next_wl` | 取出下一条数据（借用内存） |

### 14.4 CDR 序列化读取

| 函数 | 说明 |
|------|------|
| `dds_peekcdr` | Peek CDR 序列化数据 |
| `dds_peekcdr_instance` | Peek 指定实例 CDR 数据 |
| `dds_readcdr` | Read CDR 序列化数据 |
| `dds_readcdr_instance` | Read 指定实例 CDR 数据 |
| `dds_takecdr` | Take CDR 序列化数据 |
| `dds_takecdr_instance` | Take 指定实例 CDR 数据 |

### 14.5 带 Collector 的读取

| 函数 | 说明 |
|------|------|
| `dds_peek_with_collector` | Peek 并调用 collector 回调 |
| `dds_read_with_collector` | Read 并调用 collector 回调 |
| `dds_take_with_collector` | Take 并调用 collector 回调 |

---

## 15. 条件与等待集

| 函数 | 签名 | 说明 |
|------|------|------|
| `dds_create_readcondition` | `(reader: dds_entity_t, mask: u32) -> dds_entity_t` | 创建读取条件 |
| `dds_create_querycondition` | `(reader: dds_entity_t, mask: u32, filter: dds_querycondition_filter_fn) -> dds_entity_t` | 创建查询条件 |
| `dds_create_guardcondition` | `(owner: dds_entity_t) -> dds_entity_t` | 创建守护条件 |
| `dds_set_guardcondition` | `(guardcond: dds_entity_t, triggered: bool) -> dds_return_t` | 设置守护条件触发状态 |
| `dds_read_guardcondition` | `(guardcond: dds_entity_t, triggered: *mut bool) -> dds_return_t` | 读取守护条件状态（不清除） |
| `dds_take_guardcondition` | `(guardcond: dds_entity_t, triggered: *mut bool) -> dds_return_t` | 读取并清除守护条件状态 |
| `dds_create_waitset` | `(owner: dds_entity_t) -> dds_entity_t` | 创建等待集 |
| `dds_waitset_get_entities` | `(waitset: dds_entity_t, entities: *mut dds_entity_t, size: usize) -> dds_return_t` | 获取等待集中的条件列表 |
| `dds_waitset_attach` | `(waitset: dds_entity_t, entity: dds_entity_t, x: dds_attach_t) -> dds_return_t` | 将条件附加到等待集 |
| `dds_waitset_detach` | `(waitset: dds_entity_t, entity: dds_entity_t) -> dds_return_t` | 从等待集移除条件 |
| `dds_waitset_set_trigger` | `(waitset: dds_entity_t, trigger: bool) -> dds_return_t` | 手动触发等待集 |
| `dds_waitset_wait` | `(waitset: dds_entity_t, xs: *mut dds_attach_t, nxs: usize, reltime: dds_duration_t) -> dds_return_t` | 等待条件触发（相对超时） |
| `dds_waitset_wait_until` | `(waitset: dds_entity_t, xs: *mut dds_attach_t, nxs: usize, abstimestamp: dds_time_t) -> dds_return_t` | 等待条件触发（绝对时间） |

---

## 16. 实例管理

| 函数 | 签名 | 说明 |
|------|------|------|
| `dds_lookup_instance` | `(entity: dds_entity_t, data: *const c_void) -> dds_instance_handle_t` | 查找实例句柄 |
| `dds_instance_get_key` | `(entity: dds_entity_t, inst: dds_instance_handle_t, data: *mut c_void) -> dds_return_t` | 通过实例句柄获取键值 |
| `dds_begin_coherent` | `(entity: dds_entity_t) -> dds_return_t` | 开始一致性写入/访问 |
| `dds_end_coherent` | `(entity: dds_entity_t) -> dds_return_t` | 结束一致性写入/访问 |
| `dds_notify_readers` | `(subscriber: dds_entity_t) -> dds_return_t` | 通知订阅者数据可用 |
| `dds_triggered` | `(entity: dds_entity_t) -> dds_return_t` | 检查实体是否已触发 |
| `dds_get_topic` | `(entity: dds_entity_t) -> dds_entity_t` | 获取读者/写者关联的 Topic |
| `dds_assert_liveliness` | `(entity: dds_entity_t) -> dds_return_t` | 手动断言写者活跃性 |

---

## 17. 内置 Topic 与匹配信息

| 函数 | 签名 | 说明 |
|------|------|------|
| `dds_get_matched_subscriptions` | `(writer: dds_entity_t, rds: *mut dds_instance_handle_t, nrds: usize) -> dds_return_t` | 获取写者匹配的订阅列表 |
| `dds_get_matched_subscription_data` | `(writer: dds_entity_t, ih: dds_instance_handle_t) -> *mut dds_builtintopic_endpoint_t` | 获取匹配订阅的端点信息 |
| `dds_get_matched_publications` | `(reader: dds_entity_t, wrs: *mut dds_instance_handle_t, nwrs: usize) -> dds_return_t` | 获取读者匹配的发布列表 |
| `dds_get_matched_publication_data` | `(reader: dds_entity_t, ih: dds_instance_handle_t) -> *mut dds_builtintopic_endpoint_t` | 获取匹配发布的端点信息 |
| `dds_builtintopic_get_endpoint_type_info` | `(builtintopic_endpoint: *mut dds_builtintopic_endpoint_t) -> *const dds_typeinfo_t` | 获取端点类型信息 |
| `dds_builtintopic_free_endpoint` | `(builtintopic_endpoint: *mut dds_builtintopic_endpoint_t)` | 释放内置 Topic 端点 |
| `dds_builtintopic_free_topic` | `(builtintopic_topic: *mut dds_builtintopic_topic_t)` | 释放内置 Topic 数据 |
| `dds_builtintopic_free_participant` | `(builtintopic_participant: *mut dds_builtintopic_participant_t)` | 释放内置 Topic 参与者数据 |
| `dds_domain_set_deafmute` | `(entity: dds_entity_t, deaf: bool, mute: bool, reset_after: dds_duration_t) -> dds_return_t` | 设置域的聋哑模式（测试用） |

---

## 18. 借用与共享内存

| 函数 | 签名 | 说明 |
|------|------|------|
| `dds_request_loan` | `(entity: dds_entity_t, sample: *mut *mut c_void) -> dds_return_t` | 向写者借用样本缓冲区 |
| `dds_return_loan` | `(entity: dds_entity_t, buf: *mut *mut c_void, bufsz: i32) -> dds_return_t` | 归还借用的样本缓冲区 |
| `dds_request_loan_of_size` | `(entity: dds_entity_t, sample_size: usize, sample: *mut *mut c_void) -> dds_return_t` | 借用指定大小的缓冲区 |
| `dds_is_loan_available` | `(entity: dds_entity_t) -> bool` | 检查实体是否支持借用 |
| `dds_is_shared_memory_available` | `(entity: dds_entity_t) -> bool` | 检查是否支持共享内存传输 |
| `dds_loan_sample` | `(entity: dds_entity_t, sample: *mut *mut c_void) -> dds_return_t` | 借用样本（共享内存用途） |

---

## 19. 动态类型

### 类型构建

| 函数 | 说明 |
|------|------|
| `dds_dynamic_type_create` | 创建动态类型 |
| `dds_dynamic_type_set_extensibility` | 设置类型可扩展性 |
| `dds_dynamic_type_set_bit_bound` | 设置位掩码位宽 |
| `dds_dynamic_type_set_nested` | 设置嵌套标志 |
| `dds_dynamic_type_set_autoid` | 设置自动 ID 模式 |
| `dds_dynamic_type_add_member` | 添加成员 |
| `dds_dynamic_type_add_enum_literal` | 添加枚举字面量 |
| `dds_dynamic_type_add_bitmask_field` | 添加位掩码字段 |

### 成员属性

| 函数 | 说明 |
|------|------|
| `dds_dynamic_member_set_key` | 设置成员为键字段 |
| `dds_dynamic_member_set_optional` | 设置成员为可选 |
| `dds_dynamic_member_set_external` | 设置成员为外部指针 |
| `dds_dynamic_member_set_hashid` | 设置成员哈希 ID |
| `dds_dynamic_member_set_must_understand` | 设置成员必须理解标志 |

### 类型引用管理

| 函数 | 签名 | 说明 |
|------|------|------|
| `dds_dynamic_type_register` | `(type_: *mut dds_dynamic_type_t, registered_type_desc: *mut *mut dds_topic_descriptor_t) -> dds_return_t` | 注册动态类型为 Topic 描述符 |
| `dds_dynamic_type_ref` | `(type_: *mut dds_dynamic_type_t) -> dds_dynamic_type_t` | 增加类型引用计数 |
| `dds_dynamic_type_unref` | `(type_: *mut dds_dynamic_type_t) -> dds_return_t` | 减少类型引用计数 |
| `dds_dynamic_type_dup` | `(src: *const dds_dynamic_type_t) -> dds_dynamic_type_t` | 复制动态类型 |

---

## 20. 类型信息

| 函数 | 签名 | 说明 |
|------|------|------|
| `dds_get_typeobj` | `(entity: dds_entity_t, type_id: *const dds_typeid_t, timeout: dds_duration_t, type_obj: *mut *mut dds_typeobj_t) -> dds_return_t` | 获取类型对象（XTypes 类型解析） |
| `dds_free_typeobj` | `(type_obj: *mut dds_typeobj_t) -> dds_return_t` | 释放类型对象 |
| `dds_get_typeinfo` | `(entity: dds_entity_t, type_info: *mut *mut dds_typeinfo_t) -> dds_return_t` | 获取实体类型信息 |
| `dds_free_typeinfo` | `(type_info: *mut dds_typeinfo_t) -> dds_return_t` | 释放类型信息 |
| `dds_get_entity_sertype` | `(entity: dds_entity_t, sertype: *mut *const ddsi_sertype) -> dds_return_t` | 获取实体的序列化类型 |

---

*共计 **326** 个外部函数，分为 **20** 个功能类别。*
