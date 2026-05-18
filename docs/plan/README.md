# Plan 索引

`docs/plan/` 只保留仍需要决策、实现或持续打磨的计划。已经落地的阶段性方案移入 `docs/plan/archive/`，作为历史设计、实现索引和回归检查材料。

## 活跃计划

| 文档 | 状态 | 下一步 |
|------|------|--------|
| [minigame-system.md](minigame-system.md) | Phase 1 已完成，Phase 2/3 活跃 | 补 Memory/Catch、游戏配置、分数持久化、AI `perform_game` / `play_game` 工具 |
| [pet-asset-packaging.md](pet-asset-packaging.md) | 新增，承接 v2 宠物资源包发布策略 | 决定哪些资源内置、哪些做外部包；补资源包体积预算和用户目录加载 |
| [progression-capability-unlock.md](progression-capability-unlock.md) | 设计落地计划，未开始实现 | 先做 ProgressStore + 对话成长上下文注入，再做工具权限 gate |
| [music-reactive-dance-research.md](music-reactive-dance-research.md) | 第一版可用，舞感状态机仍活跃 | 扩展音乐状态机、fake source 模式、后端特征字段和调参入口 |
| [claude-code-agent-watch.md](claude-code-agent-watch.md) | 设计草案，Claude Code hook MVP 已部分落地 | 继续完善只读 Hook、会话状态收敛和 Agent Watch 展示 |
| [topdown-rpg-ai.md](topdown-rpg-ai.md) | 规划中，未开始 | 俯视角 RPG + AI NPC 对话 / AI 关卡生成，Phase 1 MVP |
| [3d-architecture.md](3d-architecture.md) | 规划中，未开始 | 等 2D 桌宠/游戏主线稳定后再评估 Three.js/voxel 迁移 |

## 已归档

| 文档 | 归档原因 |
|------|----------|
| [archive/pet-animation-visual-roadmap.md](archive/pet-animation-visual-roadmap.md) | 前端真实状态机测试、语义状态帧、表演优先级和 idle variants 主线已落地或被 v2 资源包主线取代 |
| [archive/pet-spritesheet-manifest.md](archive/pet-spritesheet-manifest.md) | v1/v2 迁移设计已完成；当前代码已进入 v2-only 资源包模式 |
| [archive/token-tracking.md](archive/token-tracking.md) | Token 明细、会话汇总和设置页统计已落地 |
| [archive/logging-standardization.md](archive/logging-standardization.md) | 第一轮日志规范化已完成，剩余作为回归检查基线 |
| [archive/rig-pet-semantic-events.md](archive/rig-pet-semantic-events.md) | PetEvent、PetEventBus、MoodPolicy、AgentReaction 和事件诊断已落地 |
| [archive/structured-output-design.md](archive/structured-output-design.md) | 舞蹈 tool-native 结构化参数已落地；游戏部分已拆到活跃 minigame 计划 |
| [archive/rig-capability-roadmap.md](archive/rig-capability-roadmap.md) | P0/P1.5/宠物语义事件主线已完成；剩余方向已并入活跃计划或 roadmap |

归档不是删除：后续需要查设计取舍、实现阶段或历史背景时，仍优先引用 archive 中的原文。

| [archive/remote-agent-monitor.md](archive/remote-agent-monitor.md) | Remote Agent Watch LAN ingest/viewer MVP ����أ��û�˵��ת�� `docs/guide/remote-agent-watch.md` |
