# Plan 索引

`docs/plan/` 只保留仍需要决策、实现或继续打磨的计划。已经落地的阶段性方案移入 `docs/plan/archive/`，作为历史设计和实现索引保留。

## 活跃计划

| 文档 | 状态 | 下一步 |
|------|------|--------|
| [minigame-system.md](minigame-system.md) | Phase 1 已完成，Phase 2/3 活跃 | 补 Memory/Catch、游戏配置/分数持久化、AI `perform_game` / `play_game` 工具 |
| [pet-animation-visual-roadmap.md](pet-animation-visual-roadmap.md) | 活跃，承接桌宠动画研究的下一步实现 | 修正前端测试真实化、补 focused/preparing 精灵、定义表演优先级 |
| [pet-spritesheet-manifest.md](pet-spritesheet-manifest.md) | Phase A/B 已落地，外部宠物资产格式继续活跃 | 做设置页选择、用户目录加载和预览诊断 |
| [music-reactive-dance-research.md](music-reactive-dance-research.md) | 第一版可用，舞感状态机仍活跃 | 扩展音乐状态机、fake source 模式、后端特征字段和调参入口 |
| [claude-code-agent-watch.md](claude-code-agent-watch.md) | 设计草案，未开始 | 先做只读 Hook MVP，观察 Claude Code 会话状态 |
| [3d-architecture.md](3d-architecture.md) | 规划中，未开始 | 等 2D 桌宠/游戏主线稳定后再评估 Three.js/voxel 迁移 |

## 已归档

| 文档 | 归档原因 |
|------|----------|
| [archive/token-tracking.md](archive/token-tracking.md) | Token 明细、会话汇总和设置页统计已落地 |
| [archive/logging-standardization.md](archive/logging-standardization.md) | 第一轮日志规范化已完成，剩余作为回归检查基线 |
| [archive/rig-pet-semantic-events.md](archive/rig-pet-semantic-events.md) | PetEvent、PetEventBus、MoodPolicy、AgentReaction 和事件诊断已落地 |
| [archive/structured-output-design.md](archive/structured-output-design.md) | 舞蹈 tool-native 结构化参数已落地；游戏部分已拆到活跃 minigame 计划 |
| [archive/rig-capability-roadmap.md](archive/rig-capability-roadmap.md) | P0/P1.5/宠物语义事件主线已完成；剩余方向已并入活跃计划或 roadmap |

归档不是删除：后续需要查设计取舍、实现阶段或历史背景时，仍优先引用 archive 中的原文。
