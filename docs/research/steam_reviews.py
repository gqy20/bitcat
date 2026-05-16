"""
Steam 评测爬取 + 分析一体脚本
用法:
  python steam_reviews.py                          # 默认: 爬中文500条 + 分析
  python steam_reviews.py --appid 3419430 --max 1000 --lang schinese
  python steam_reviews.py --analyze-only            # 只分析已有数据 (bongo_cat_reviews.jsonl)
  python steam_reviews.py --scrape-only             # 只爬取不分析

输出:
  docs/research/<appid>_reviews.jsonl   原始数据 (可 grep)
  终端                              分析报告
"""

import json
import re
import time
import argparse
import requests
from collections import Counter
from pathlib import Path

DATA_DIR = Path(__file__).parent
HEADERS = {"User-Agent": "Mozilla/5.0 (Windows NT 10.0; Win64; x64)"}


# ── 爬取 ──────────────────────────────────────────────

def scrape(appid: int, lang: str = "schinese", max_n: int = 500) -> list[dict]:
    base = f"https://store.steampowered.com/appreviews/{appid}"
    reviews, cursor, fetched = [], "*", 0
    print(f"[爬取] appid={appid} lang={lang} max={max_n}")

    while fetched < max_n:
        try:
            r = requests.get(base, params={
                "json": 1, "review_type": "all", "purchase_type": "all",
                "language": lang, "num_per_page": min(100, max_n - fetched), "cursor": cursor,
            }, headers=HEADERS, timeout=15).json()
        except Exception as e:
            print(f"  [ERR] {e}, 重试...")
            time.sleep(3)
            try:
                r = requests.get(base, params={
                    "json": 1, "review_type": "all", "purchase_type": "all",
                    "language": lang, "num_per_page": min(100, max_n - fetched), "cursor": cursor,
                }, headers=HEADERS, timeout=15).json()
            except Exception:
                break

        if not r.get("success"):
            break
        batch = r.get("reviews", [])
        if not batch:
            break
        reviews.extend(batch)
        fetched += len(batch)
        cursor = r.get("cursor", "*")
        print(f"  {fetched}/{max_n}")
        if cursor in ("*", ""):
            break
        time.sleep(1.2)

    return reviews


def clean(r: dict) -> dict:
    a = r.get("author", {})
    return {
        "review_id": r.get("recommendationid"),
        "playtime_forever": round(a.get("playtime_forever", 0) / 60, 1),
        "playtime_at_review": round(a.get("playtime_at_review", 0) / 60, 1),
        "voted_up": r.get("voted_up"),
        "votes_up": r.get("votes_up"),
        "language": r.get("language"),
        "text": r.get("review", ""),
        "ts_created": r.get("timestamp_created"),
        "num_games_owned": a.get("num_games_owned"),
        "num_reviews": a.get("num_reviews"),
    }


def save(reviews: list[dict], path: Path):
    cleaned = [clean(r) for r in reviews]
    path.parent.mkdir(parents=True, exist_ok=True)
    with open(path, "w", encoding="utf-8") as f:
        for r in cleaned:
            f.write(json.dumps(r, ensure_ascii=False) + "\n")
    print(f"[保存] {len(cleaned)} 条 → {path}")


# ── 分析 ──────────────────────────────────────────────

def analyze(reviews: list[dict], label: str):
    n = len(reviews)
    up = sum(1 for r in reviews if r.get("voted_up"))
    hours = sorted((r.get("playtime_forever", 0) for r in reviews), reverse=True)
    games = [r.get("num_games_owned", 0) for r in reviews if r.get("num_games_owned")]

    print(f"\n{'─'*50}")
    print(f"  {label} ({n} 条)")
    print(f"{'─'*50}")
    print(f"  推荐: {up}/{n} ({up/n*100:.1f}%)" if n else "")
    if hours:
        print(f"  时长: 中位 {hours[n//2]:.0f}h | 均 {sum(hours)/n:.0f}h | 最高 {hours[0]:.0f}h")
    if games:
        print(f"  拥有游戏: 均 {sum(games)/len(games):.0f} 款")

    # 关键词
    text = " ".join(r.get("text", "") or "" for r in reviews)
    kw_patterns = [
        ("可爱/治愈/萌", [r"可爱", r"治愈", r"萌"]),
        ("猫/挂机/工作", [r"猫", r"挂机", r"工作", r"摸鱼"]),
        ("推荐/好用", [r"推荐", r"不错", r"很好", r"方便", r"实用"]),
        ("皮肤/DLC/付费", [r"皮肤", r"DLC", r"氪金", r"付费", r"内购"]),
        ("性能问题", [r"内存", r"占用", r"卡顿", r"崩溃", r"闪退", r"CPU"]),
        ("内容单调", [r"无聊", r"重复", r"单调", r"没意思"]),
        ("Bug", [r"bug", r"BUG", r"错误", r"修复"]),
        ("功能建议", [r"希望", r"建议", r"求", r"加个", r"新增"]),
    ]
    print(f"\n  关键词:")
    for group, pats in kw_patterns hits := [(p, len(re.findall(p, text, re.I))) for p in pats]:
        total = sum(hits)
    if total > 0:
        top = sorted(zip(pats, hits), key=lambda x: -x[1])[:3]
        print(f"    {group:<12} {' + '.join(f'{k}({v})' for k,v in top)}")

    # 典型评测
    top_pos = sorted(((r, r.get("votes_up", 0)) for r in reviews if r.get("voted_up")), key=lambda x: -x[1])[:3]
    negs = [r for r in reviews if not r.get("voted_up")][:3]

    print(f"\n  高赞推荐:")
    for r, v in top_pos:
        t = (r.get("text") or "")[:100]
        print(f"    [+{v}|{r.get('playtime_forever',0):.0f}h] {t}")

    if negs:
        print(f"\n  差评 ({len([r for r in reviews if not r.get('voted_up')])} 条):")
        for r in negs:
            t = (r.get("text") or "")[:120]
            print(f"    [{r.get('playtime_forever',0):.0f}h] {t}")


# ── main ───────────────────────────────────────────────

def main():
    p = argparse.ArgumentParser(description="Steam 评测爬取+分析")
    p.add_argument("--appid", type=int, default=3419430)
    p.add_argument("--lang", default="schinese")
    p.add_argument("--max", type=int, default=500)
    p.add_argument("--output", default=None)
    p.add_argument("--analyze-only", action="store_true")
    p.add_argument("--scrape-only", action="store_true")
    args = p.parse_args()

    out = Path(args.output) if args.output else DATA_DIR / f"{args.appid}_reviews.jsonl"

    if not args.analyze_only:
        t0 = time.time()
        raw = scrape(args.appid, args.lang, args.max)
        save(raw, out)
        print(f"  耗时 {time.time()-t0:.1f}s")

    if not args.scrape_only:
        with open(out, "r", encoding="utf-8") as f:
            data = [json.loads(line) for line in f if line.strip()]
        analyze(data, args.lang)


if __name__ == "__main__":
    main()
