#!/usr/bin/env python3
"""从场景源码里抽出卡片表，生成 docs/STORYBOARD.md。

为什么不手抄：一支曲子有 90 多张卡，手抄必然和代码脱节。
这里直接解析 `scenes_*.rs` 的卡表（两种写法都支持），
时间 = 段起点（来自 assets/timeline.toml） + 卡表里的段内偏移。

用法：python3 scripts/gen_storyboard.py
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent

# 段落顺序与显示名（与 src/timeline/scene.rs 的 SceneKind::ALL 一致）
SEGMENTS = [
    ("intro", "序", "晨雾未醒"),
    ("spring", "春", "苔露蝶蕾"),
    ("summer", "夏", "白雨莲叶"),
    ("bridge", "橋", "瞬き・幻"),
    ("chorus", "副歌", "空の気配と人の世"),
    ("autumn", "秋", "松影月盃"),
    ("winter", "冬", "枯野银花"),
    ("ascension", "昇華", "绘卷独楽"),
    ("finale", "終章", "记忆不锈"),
    ("outro", "尾声", "收卷"),
]

# 场景名 → 源码文件
FILES = {
    "intro": "scenes_intro.rs",
    "spring": "scenes_spring.rs",
    "summer": "scenes_summer.rs",
    "bridge": "scenes_bridge.rs",
    "chorus": "scenes_chorus.rs",
    "autumn": "scenes_autumn.rs",
    "winter": "scenes_winter.rs",
    "ascension": "scenes_ascension.rs",
    "finale": "scenes_ascension.rs",  # 与昇華同文件，靠卡表变量区分
    "outro": "scenes_outro.rs",
}

# 元组写法：(0.0, "moss_awakens", card_moss_awakens),
TUPLE_RE = re.compile(r'\(\s*(\d+(?:\.\d+)?)\s*,\s*"([^"]+)"\s*,')
# 结构体写法：XxxCard { at: 0.0, name: "松ノ影", draw: f_01 },
STRUCT_RE = re.compile(r'at:\s*(\d+(?:\.\d+)?)\s*,\s*name:\s*"([^"]+)"')

# 昇華/終章在同一文件里，用卡表常量名切分
ASC_MARKERS = {
    "ascension": ("ASC_CARDS", "FIN_CARDS"),
    "finale": ("FIN_CARDS", None),
}


def segment_starts() -> dict[str, float]:
    """从 assets/timeline.toml 读段起点。"""
    text = (ROOT / "assets" / "timeline.toml").read_text(encoding="utf-8")
    starts: dict[str, float] = {}
    for m in re.finditer(r'scene\s*=\s*"([a-z]+)"\s*\n\s*start\s*=\s*([0-9.]+)', text):
        starts[m.group(1)] = float(m.group(2))
    return starts


def split_ascension(text: str) -> tuple[str, str]:
    """把 scenes_ascension.rs 按 ASC_CARDS / FIN_CARDS 切成两段。"""
    i = text.find("ASC_CARDS")
    j = text.find("FIN_CARDS")
    if i < 0 or j < 0:
        return text, text
    if i < j:
        return text[i:j], text[j:]
    return text[j:i], text[i:]


def cards_of(scene: str) -> list[tuple[float, str]]:
    path = ROOT / "src" / "render" / FILES[scene]
    if not path.exists():
        return []
    text = path.read_text(encoding="utf-8")

    if scene in ASC_MARKERS:
        a, b = split_ascension(text)
        text = a if scene == "ascension" else b

    found = [(float(t), n) for t, n in TUPLE_RE.findall(text)]
    if not found:
        found = [(float(t), n) for t, n in STRUCT_RE.findall(text)]
    # 去重并保持出现顺序（正则可能重复命中同一行）
    seen = set()
    out = []
    for t, n in found:
        if (t, n) in seen:
            continue
        seen.add((t, n))
        out.append((t, n))
    out.sort(key=lambda x: x[0])
    return out


def fmt_time(sec: float) -> str:
    m = int(sec // 60)
    s = sec - m * 60
    return f"{m}:{s:05.2f}"


def main() -> int:
    starts = segment_starts()
    lines: list[str] = []
    lines.append("# 分镜表 —《春・夏・秋・冬》")
    lines.append("")
    lines.append("> 本文件由 `scripts/gen_storyboard.py` 从场景源码的卡表自动生成，**不要手改**。")
    lines.append("> 时间 = 段起点（`assets/timeline.toml`）+ 卡表内的段内偏移。")
    lines.append("")

    total = 0
    summary: list[tuple[str, str, int, str]] = []
    bodies: list[str] = []

    for key, kanji, subtitle in SEGMENTS:
        cards = cards_of(key)
        start = starts.get(key, 0.0)
        total += len(cards)
        span = "—"
        if cards:
            span = f"{cards[0][0]:.1f}s ~ {cards[-1][0]:.1f}s（段内）"
        summary.append((kanji, subtitle, len(cards), span))

        body = [f"## {kanji}　{subtitle}", ""]
        body.append(f"- 段起点：**{fmt_time(start)}**（{start:.2f}s）")
        body.append(f"- 卡片数：**{len(cards)}**")
        body.append("")
        if not cards:
            body.append("_（该段尚未接线卡片表）_")
            body.append("")
            bodies.append("\n".join(body))
            continue
        body.append("| # | 段内 | 绝对时间 | 卡名 |")
        body.append("|---|---|---|---|")
        for i, (off, name) in enumerate(cards, 1):
            body.append(f"| {i:02d} | {off:.2f}s | {fmt_time(start + off)} | `{name}` |")
        body.append("")
        bodies.append("\n".join(body))

    lines.append("## 总览")
    lines.append("")
    lines.append("| 段落 | 主题 | 卡片数 | 段内跨度 |")
    lines.append("|---|---|---|---|")
    for kanji, subtitle, n, span in summary:
        lines.append(f"| {kanji} | {subtitle} | **{n}** | {span} |")
    lines.append(f"| **合计** | | **{total}** | |")
    lines.append("")
    lines.extend(bodies)

    out = ROOT / "docs" / "STORYBOARD.md"
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text("\n".join(lines) + "\n", encoding="utf-8")

    # 给调用方一个可判定的摘要
    print(f"已写入 {out}")
    for kanji, subtitle, n, _ in summary:
        flag = "" if n >= 6 else "  <-- 偏少"
        print(f"  {kanji:<4} {subtitle:<20} {n:>3} 张{flag}")
    print(f"  合计 {total} 张")
    return 0


if __name__ == "__main__":
    sys.exit(main())
