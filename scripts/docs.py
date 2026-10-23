#!/usr/bin/env python3
"""Offline documentation catalog, bounded search, and repository link checks (stdlib only)."""

import argparse
import hashlib
import json
import re
import sys
import tempfile
import unicodedata
from pathlib import Path
from urllib.parse import unquote, urlsplit

ROOT = Path(__file__).resolve().parents[1]
CATALOG = "docs/catalog.json"
INDEX = "docs/architecture/catalog.md"
TOPICS = "docs/architecture/topics.json"
ROLES = {
    "Prima OS.md": "architecture-contract",
    "COSIR 原语规范.md": "abstract-semantics",
    "Prima OS 实现阶段.md": "current-status",
    "lean/README.md": "lean-model-mapping",
    "docs/superpowers/compat/compat-state.md": "compatibility-ledger",
    "docs/superpowers/compat/symbol-trace.md": "symbol-trace",
}


def source_files():
    """Intentionally avoid target/, vendored dependencies, and private editor directories."""
    paths = list(ROOT.glob("*.md")) + list((ROOT / "docs").rglob("*.md"))
    paths += list((ROOT / "lean").glob("*.md"))
    return sorted(p for p in paths if p.relative_to(ROOT).as_posix() != INDEX)


def prose_lines(source):
    """Preserve source line numbers while excluding fenced code and front matter."""
    fence = None
    front = source.startswith("---\n")
    for number, line in enumerate(source.splitlines(), 1):
        if front:
            if number > 1 and line == "---":
                front = False
            continue
        marker = re.match(r"^\s{0,3}(`{3,}|~{3,})", line)
        if marker:
            token = marker[1]
            if fence is None:
                fence = token
            elif token[0] == fence[0] and len(token) >= len(fence):
                fence = None
            continue
        if fence is None:
            yield number, line


def metadata(source):
    if not source.startswith("---\n"):
        return {}
    header = source.split("---", 2)[1]
    return dict(re.findall(r"^([a-z_]+): ([^\n]+)$", header, re.M))


def slug(title):
    title = re.sub(r"\[([^]]+)\]\([^)]*\)", r"\1", title)
    title = re.sub(r"<[^>]*>", "", title).strip().lower()
    return "".join(
        "-" if c.isspace() else c
        for c in title
        if c.isspace() or c in "_-" or unicodedata.category(c)[0] in "LN"
    )


def headings(source):
    found, seen = [], {}
    for number, line in prose_lines(source):
        match = re.match(r"^(#{1,6})\s+(.+?)(?:\s+#+)?$", line)
        if not match:
            continue
        base = slug(match[2])
        occurrence = seen.get(base, 0)
        seen[base] = occurrence + 1
        anchor = base + (f"-{occurrence}" if occurrence else "")
        found.append({"level": len(match[1]), "title": match[2],
                      "line": number, "anchor": anchor})
    total = len(source.splitlines())
    for index, section in enumerate(found):
        section["end_line"] = next(
            (s["line"] - 1 for s in found[index + 1:] if s["level"] <= section["level"]),
            total,
        )
    return found


def links(source):
    for number, line in prose_lines(source):
        line = re.sub(r"`+[^`\n]*`+", "", line)
        for match in re.finditer(r"\]\((<[^>]+>|[^)\n]+)\)", line):
            target = re.sub(r'\s+["\'].*$', "", match[1].strip()).strip("<>")
            yield number, target
        definition = re.match(r"^\s{0,3}\[[^]]+\]:\s*(<[^>]+>|\S+)", line)
        if definition:
            yield number, definition[1].strip("<>")


def resolve(path, target):
    parts = urlsplit(target)
    if parts.scheme or parts.netloc:
        return None, None
    name = unquote(parts.path)
    destination = (ROOT / name.lstrip("/") if name.startswith("/") else path.parent / name)
    return (destination if name else path).resolve(), unquote(parts.fragment)


def kind(path):
    if path.name == "AGENTS.md":
        return "agent-instructions"
    if path.name == "README.md":
        return "index"
    for part, value in (("specs", "specification"), ("plans", "plan"),
                        ("reports", "report"), ("compat", "compatibility")):
        if part in path.parts:
            return value
    return "guide"


def collect():
    topics = json.loads((ROOT / TOPICS).read_text(encoding="utf-8"))
    if not isinstance(topics, list):
        raise ValueError(f"{TOPICS}: expected a list of topic objects")
    for index, topic in enumerate(topics):
        if not isinstance(topic, dict):
            raise ValueError(f"{TOPICS}[{index}]: expected an object")
        for field in ("id", "title", "guide", "aliases", "sources", "code", "tests"):
            value = topic.get(field)
            valid = isinstance(value, str) and bool(value)
            if field in ("aliases", "sources", "code", "tests"):
                valid = isinstance(value, list) and all(isinstance(v, str) and v for v in value)
            if not valid:
                raise ValueError(f"{TOPICS}[{index}]: invalid or missing field: {field}")
    documents = []
    for path in source_files():
        source = path.read_text(encoding="utf-8")
        relative = path.relative_to(ROOT).as_posix()
        meta, sections = metadata(source), headings(source)
        documents.append({
            "path": relative, "id": meta.get("id", relative),
            "title": sections[0]["title"] if sections else path.stem,
            "kind": meta.get("kind", kind(path)),
            "authority": ROLES.get(relative, meta.get("authority", "scope-defined-in-document")),
            "status": meta.get("status", "read-document-for-status"),
            "date": meta.get("updated", re.match(r"\d{4}-\d{2}-\d{2}", path.name)[0]
                             if re.match(r"\d{4}-\d{2}-\d{2}", path.name) else None),
            "sha256": hashlib.sha256(source.encode()).hexdigest(),
            "topic_ids": [t["id"] for t in topics
                          if relative == t["guide"] or relative in t["sources"]],
            "lines": len(source.splitlines()), "sections": sections,
            "mentioned_ids": sorted(set(re.findall(
                r"\b(?:SPEC-\d+|DOC-\d+|BLK-\d+|ISSUE-[A-Z0-9-]+|S[0-7]-[DX]\d+)\b", source))),
            "links": [{"line": n, "target": t} for n, t in links(source)],
        })
    return {"schema_version": 1, "path_base": "repository-root",
            "notice": "Dates and mentioned_ids are retrieval hints, never approval or implementation status.",
            "topics": topics,
            "documents": documents}


def products(catalog):
    from os.path import relpath
    from urllib.parse import quote

    rows = ["# 全量文档目录", "", "> 自动生成：`python3 scripts/docs.py build`。勿手改。",
            "> 范围：仓库根目录、docs/、lean/ 的 Markdown 源文档；本生成页不自索引。",
            "", "返回[文档入口](../README.md) · [主题地图](topics/README.md) · "
            "[机器目录](../catalog.json)", "",
            "文件日期用于定位历史，不能用于推断当前实现或批准状态。", ""]
    for category in sorted({d["kind"] for d in catalog["documents"]}):
        selected = [d for d in catalog["documents"] if d["kind"] == category]
        rows += [f"## {category}（{len(selected)}）", "", "| 文档 | 路径 |", "|---|---|"]
        for document in selected:
            target = quote(relpath(ROOT / document["path"], (ROOT / INDEX).parent), safe="/-._")
            title = document["title"].replace("|", "\\|")
            rows.append(f"| [{title}]({target}) | `{document['path']}` |")
        rows.append("")
    return {CATALOG: json.dumps(catalog, ensure_ascii=False, indent=2) + "\n",
            INDEX: "\n".join(rows)}


def validate(catalog):
    errors, ids = [], set()
    documents = {d["path"]: d for d in catalog["documents"]}
    for document in documents.values():
        path = ROOT / document["path"]
        if document["id"] in ids:
            errors.append(f"duplicate document id: {document['id']}")
        ids.add(document["id"])
        if not document["sections"]:
            errors.append(f"{document['path']}: missing Markdown heading")
        source = path.read_text(encoding="utf-8")
        meta = metadata(source)
        if meta:
            for field in ("id", "kind", "authority", "status", "updated"):
                if not meta.get(field):
                    errors.append(f"{document['path']}: missing metadata field: {field}")
            if not re.fullmatch(r"\d{4}-\d{2}-\d{2}", meta.get("updated", "")):
                errors.append(f"{document['path']}: updated must be YYYY-MM-DD")
        for number, line in prose_lines(source):
            if re.search(r"\[\[[^]\n]+\]\]", re.sub(r"`[^`]*`", "", line)):
                errors.append(f"{document['path']}:{number}: unresolved wiki link")
        for link in document["links"]:
            destination, anchor = resolve(path, link["target"])
            if destination is None:
                continue
            location = f"{document['path']}:{link['line']}"
            if not destination.is_relative_to(ROOT):
                errors.append(f"{location}: link escapes repository: {link['target']}")
            elif not destination.exists():
                errors.append(f"{location}: missing target: {link['target']}")
            elif anchor and destination.suffix == ".md":
                source = destination.read_text(encoding="utf-8")
                anchors = {s["anchor"] for s in headings(source)}
                anchors.update(re.findall(r'<a\s+(?:id|name)=["\']([^"\']+)', source))
                if anchor not in anchors:
                    errors.append(f"{location}: missing anchor: {link['target']}")
    for directory in ("specs", "plans", "reports"):
        folder = ROOT / "docs/superpowers" / directory
        index = folder / "README.md"
        if not folder.exists():
            continue
        indexed = {resolve(index, target)[0] for _, target in links(index.read_text(encoding="utf-8"))}
        for path in folder.glob("*.md"):
            if path.name != "README.md" and path.resolve() not in indexed:
                errors.append(f"{path.relative_to(ROOT)}: missing from directory README")
    topic_ids = set()
    for topic in catalog["topics"]:
        if topic["id"] in topic_ids:
            errors.append(f"duplicate topic id: {topic['id']}")
        topic_ids.add(topic["id"])
        if not topic.get("aliases"):
            errors.append(f"topic {topic['id']}: missing retrieval aliases")
        for field in ("guide", "sources", "code", "tests"):
            values = [topic[field]] if field == "guide" else topic[field]
            for value in values:
                destination, anchor = resolve(ROOT / "README.md", value)
                if destination is None or not destination.is_relative_to(ROOT) or not destination.exists():
                    errors.append(f"topic {topic['id']}: missing {field}: {value}")
                elif field in ("guide", "sources") and destination.relative_to(ROOT).as_posix() not in documents:
                    errors.append(f"topic {topic['id']}: unindexed source: {value}")
    return errors


def search(catalog, query, limit, category):
    terms = query.casefold().split()
    results = []
    for document in catalog["documents"]:
        if category and document["kind"] != category:
            continue
        source = (ROOT / document["path"]).read_text(encoding="utf-8")
        text = source.casefold()
        if not all(term in text or term in document["path"].casefold() for term in terms):
            continue
        score = sum(12 * document["title"].casefold().count(t)
                    + 5 * document["path"].casefold().count(t)
                    + min(text.count(t), 8) for t in terms)
        matches = [{"line": n, "text": line.strip()[:240]}
                   for n, line in prose_lines(source) if any(t in line.casefold() for t in terms)][:4]
        results.append({"path": document["path"], "title": document["title"],
                        "kind": document["kind"], "authority": document["authority"],
                        "topic_ids": document["topic_ids"],
                        "score": score, "matches": matches})
    return sorted(results, key=lambda d: (-d["score"], d["path"]))[:limit]


def self_test():
    global ROOT
    original = ROOT
    try:
        with tempfile.TemporaryDirectory() as temp:
            ROOT = Path(temp)
            (ROOT / "docs/architecture").mkdir(parents=True)
            (ROOT / TOPICS).write_text("[]", encoding="utf-8")
            (ROOT / "README.md").write_text(
                '# 测试\n\n## Fork `API`\n[ok](#fork-api)\n## Fork `API`\n'
                '[dup](#fork-api-1)\n```md\n[ignore](missing.md)\n# Fake\n```\n'
                '[unicode](docs/%E4%B8%AD%E6%96%87.md#%E5%86%85%E5%AD%98)\n', encoding="utf-8")
            (ROOT / "docs/中文.md").write_text("# 内存\n\nCOW fork", encoding="utf-8")
            catalog = collect()
            assert not validate(catalog), validate(catalog)
            assert len(catalog["documents"][0]["sections"]) == 3
            assert search(catalog, "COW fork", 2, None)[0]["path"] == "docs/中文.md"
            with (ROOT / "README.md").open("a", encoding="utf-8") as file:
                file.write("[bad](missing.md)\n[bad anchor](#absent)\n")
            errors = validate(collect())
            assert len(errors) == 2, errors
            (ROOT / "docs/中文.md").write_text(
                "---\nid: shared\nkind: guide\n---\n# 内存\n", encoding="utf-8")
            assert any("missing metadata field" in e for e in validate(collect()))
            (ROOT / "docs/other.md").write_text(
                "---\nid: shared\nkind: guide\n---\n# Other\n", encoding="utf-8")
            assert any("duplicate document id" in e for e in validate(collect()))
            first = products(collect())
            assert first == products(collect()), "catalog must be reproducible"
    finally:
        ROOT = original
    print("PASS docs self-test: Unicode, fences, headings, links, metadata, duplicate IDs, search, determinism")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    for command in ("build", "check", "self-test"):
        commands.add_parser(command)
    search_parser = commands.add_parser("search")
    search_parser.add_argument("query")
    search_parser.add_argument("--limit", type=int, default=8)
    search_parser.add_argument("--kind")
    search_parser.add_argument("--json", action="store_true")
    topic_parser = commands.add_parser("topic")
    topic_parser.add_argument("id")
    args = parser.parse_args()
    if args.command == "self-test":
        self_test()
        return 0
    catalog = collect()
    if args.command == "search":
        if not 1 <= args.limit <= 100:
            parser.error("--limit must be between 1 and 100")
        if not args.query.strip():
            parser.error("query must not be empty")
        results = search(catalog, args.query, args.limit, args.kind)
        if args.json:
            print(json.dumps(results, ensure_ascii=False, indent=2))
        else:
            for result in results:
                topics = ",".join(result["topic_ids"]) or "unassigned"
                print(f"{result['path']} [{result['kind']}; {result['authority']}; topics={topics}] {result['title']}")
                for match in result["matches"]:
                    print(f"  L{match['line']}: {match['text']}")
        return 0
    if args.command == "topic":
        topic = next((t for t in catalog["topics"] if t["id"] == args.id), None)
        if topic is None:
            parser.error("unknown topic; choose: " + ", ".join(t["id"] for t in catalog["topics"]))
        print(json.dumps(topic, ensure_ascii=False, indent=2))
        return 0
    if args.command == "build":
        for name, content in products(catalog).items():
            (ROOT / name).write_text(content, encoding="utf-8")
        print(f"Built {len(catalog['documents'])} documents, {sum(len(d['sections']) for d in catalog['documents'])} sections")
        return 0
    errors = validate(catalog)
    # Validate the generated human index too, without introducing a self-hash dependency.
    for auxiliary in (INDEX, "llms.md"):
        if not (ROOT / auxiliary).exists():
            continue
        for number, target in links((ROOT / auxiliary).read_text(encoding="utf-8")):
            destination, _ = resolve(ROOT / auxiliary, target)
            if destination is not None and not destination.exists():
                errors.append(f"{auxiliary}:{number}: missing target: {target}")
    for name, expected in products(catalog).items():
        if not (ROOT / name).exists() or (ROOT / name).read_text(encoding="utf-8") != expected:
            errors.append(f"{name}: stale; run python3 scripts/docs.py build")
    for error in errors:
        print(error, file=sys.stderr)
    if errors:
        print(f"FAIL docs: {len(errors)} error(s)", file=sys.stderr)
        return 1
    print(f"PASS docs: {len(catalog['documents'])} documents; catalog, IDs, topics, local links and Markdown anchors")
    return 0


if __name__ == "__main__":
    try:
        sys.exit(main())
    except (OSError, ValueError) as error:
        print(f"ERROR docs: {error}", file=sys.stderr)
        sys.exit(1)
