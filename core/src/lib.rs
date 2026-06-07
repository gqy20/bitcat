//! bitcat-core：BitCat 桌面 AI 伙伴的纯逻辑层。
//!
//! 本 crate 封装了 AI Agent 对话、宠物状态机、按键映射、截图分析、记忆系统等
//! 全部业务逻辑，不依赖任何 UI 或窗口框架，可独立进行单元测试。
//!
//! 之所以把逻辑从 app crate 中剥离，是为了让核心能力在脱离 SDL2 / Tauri /
//! WebView2 等平台组件时仍可编译和测试，显著加快迭代速度。
//!
//! app crate 通过 `use bitcat_core::...` 引入数据结构和函数，负责窗口管理、
//! 手柄输入循环、IPC 通信等平台相关职责；core 自身不感知窗口或渲染。
//!
//! 核心子模块概览：
//! - `agent` — 基于 rig-core 的 AI 流式对话与工具注册
//! - `agent_session` / `claude_code` / `agent_nudge` — 外部 Claude Code 看管与提醒策略
//! - `bridge` — 手柄按键 → Agent 命令 → 宠物动画的桥接映射
//! - `pet` — 宠物状态机（6 状态、帧动画）
//! - `memory` — 滚动窗口对话记忆
//! - `minigame` — 迷你游戏定义、参数边界和预设
//! - `vision` / `screenshot` — 截图捕获与 Vision API 分析
//! - `prompts` — 统一提示词配置
//! - `user_profile` — 用户画像
//! - `action` / `hotkey` — 快捷键与动作定义、Win32 SendInput 模拟
//! - `ai_config` — AI 模型密钥、base URL、模型名等运行时配置
//! - `app_settings` — 应用全局设置（窗口位置、截图间隔等）
//! - `config` — 通用配置文件加载基础设施
//! - `dance` — 舞蹈编排与播放
//! - `device` — 手柄设备枚举与连接管理
//! - `logging` — 日志工具函数（log_preview 等）
//! - `permission_hook` — rig-core 工具调用的权限拦截钩子
//! - `panel_action` — 弹出面板快捷入口配置
//! - `performance` — 表现会话状态，统一舞蹈、音乐响应与游戏接管
//! - `screen_summary` — 截图摘要注入 prompt 构建
//! - `tool_events` — 工具运行时事件审计日志
//! - `token_tracker` — Token 用量统计与持久化
//! - `tools` — 内置工具的参数类型与执行逻辑

pub mod action;
pub mod agent;
pub mod agent_nudge;
pub mod agent_reaction;
pub mod agent_session;
pub mod ai_config;
pub mod app_settings;
pub mod bridge;
pub mod camera_observation;
pub mod claude_code;
pub mod config;
pub mod dance;
pub mod device;
pub mod game_projection;
pub mod game_request;
pub mod gomoku_ai;
pub mod hotkey;
pub mod logging;
pub mod memory;
pub mod minigame;
pub mod mood_policy;
pub mod panel_action;
pub mod performance;
pub mod permission_hook;
pub mod pet;
pub mod pet_event;
pub mod points;
pub mod prompts;
pub mod reminder;
pub mod reminder_personalizer;
pub mod screen_summary;
pub mod screenshot;
#[cfg(test)]
mod screenshot_tests;
pub mod storage;
pub mod token_tracker;
pub mod tool_events;
pub mod tools;
pub mod user_profile;
pub mod vision;
pub mod vocab;
