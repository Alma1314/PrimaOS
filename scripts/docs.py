#!/usr/bin/env python3
"""Offline documentation catalog, bounded search, and repository link checks (stdlib only)."""

import argparse
from datetime import date
import hashlib
import json
import re
import subprocess
import sys
import tempfile
import unicodedata
from pathlib import Path
from urllib.parse import unquote, urlsplit

ROOT = Path(__file__).resolve().parents[1]
CATALOG = "docs/catalog.json"
INDEX = "docs/architecture/catalog.md"
TOPICS = "docs/architecture/topics.json"
LIFECYCLE = "docs/lifecycle.json"
STATES = {"current", "historical", "deprecated", "removed", "unclassified"}
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
    lines = source.splitlines()
    try:
        end = lines.index("---", 1)
    except ValueError:
        raise ValueError("unterminated Markdown front matter") from None
    fields = {}
    for line in lines[1:end]:
        if not line.strip():
            continue
        match = re.fullmatch(r"([a-z_]+): (\S.*)", line)
        if not match or match[1] in fields:
            raise ValueError(f"invalid or duplicate flat metadata field: {line}")
        fields[match[1]] = match[2]
    return fields


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


def lifecycle_records():
    data = json.loads((ROOT / LIFECYCLE).read_text(encoding="utf-8"))
    if (not isinstance(data, dict) or data.get("schema_version") != 1
            or not isinstance(data.get("entries"), list)):
        raise ValueError(f"{LIFECYCLE}: expected schema_version 1 and entries list")
    records = {}
    for entry in data["entries"]:
        if not isinstance(entry, dict):
            raise ValueError(f"{LIFECYCLE}: entry must be an object")
        if set(entry) - {"path", "lifecycle", "id", "updated", "reason", "replacements", "revision", "sha256"}:
            raise ValueError(f"{LIFECYCLE}: unknown entry field")
        path = entry.get("path", "")
        if (not isinstance(path, str) or not path or path.startswith("/") or "\\" in path
                or ".." in path.split("/") or Path(path).as_posix() != path or not path.endswith(".md")):
            raise ValueError(f"{LIFECYCLE}: invalid repository Markdown path: {path}")
        if path in records:
            raise ValueError(f"{LIFECYCLE}: duplicate path: {path}")
        if not isinstance(entry.get("lifecycle"), str) or entry["lifecycle"] not in STATES - {"unclassified"}:
            raise ValueError(f"{path}: invalid lifecycle")
        if not isinstance(entry.get("reason"), str) or not entry["reason"].strip():
            raise ValueError(f"{path}: lifecycle requires reason")
        try:
            value = entry.get("updated", "")
            if not isinstance(value, str) or not re.fullmatch(r"\d{4}-\d{2}-\d{2}", value):
                raise ValueError("invalid date")
            date.fromisoformat(value)
        except (TypeError, ValueError):
            raise ValueError(f"{path}: lifecycle updated must be YYYY-MM-DD") from None
        successors = entry.get("replacements")
        if not isinstance(successors, list) or any(not isinstance(p, str) or not p for p in successors):
            raise ValueError(f"{path}: replacements must be a list of paths")
        if len(successors) != len(set(successors)):
            raise ValueError(f"{path}: duplicate replacement")
        if entry["lifecycle"] in {"removed", "deprecated"} and not successors:
            raise ValueError(f"{path}: retirement requires replacement or historical-context entry")
        if entry["lifecycle"] == "removed":
            if (not isinstance(entry.get("id"), str) or not entry["id"]
                    or not isinstance(entry.get("revision"), str)
                    or not re.fullmatch(r"[0-9a-f]{40}", entry["revision"])
                    or not isinstance(entry.get("sha256"), str)
                    or not re.fullmatch(r"[0-9a-f]{64}", entry["sha256"])):
                raise ValueError(f"{path}: removal requires reserved id, full Git revision and sha256")
        records[path] = entry
    return records


def git(*args):
    result = subprocess.run(["git", "-C", str(ROOT), *args], capture_output=True)
    if result.returncode:
        raise ValueError(result.stderr.decode("utf-8", errors="replace").strip())
    return result.stdout


def validate_lifecycle(catalog):
    errors = []
    documents = {d["path"]: d for d in catalog["documents"]}
    reserved = {d["id"] for d in documents.values()}
    records = lifecycle_records()
    for path, entry in records.items():
        state = entry["lifecycle"]
        if state == "removed":
            if (ROOT / path).exists():
                errors.append(f"{path}: removed path must not exist")
            if entry["id"] in reserved:
                errors.append(f"{path}: retired id is reused: {entry['id']}")
            reserved.add(entry["id"])
            try:
                raw = git("show", f"{entry['revision']}:{path}")
                if hashlib.sha256(raw).hexdigest() != entry["sha256"]:
                    errors.append(f"{path}: recovery sha256 mismatch")
                if metadata(raw.decode("utf-8")).get("id", path) != entry["id"]:
                    errors.append(f"{path}: recovery id mismatch")
            except (ValueError, UnicodeError) as error:
                errors.append(f"{path}: cannot recover retired source: {error}")
        elif path not in documents:
            errors.append(f"{path}: lifecycle source is missing or unindexed")
        for replacement in entry["replacements"]:
            successor = documents.get(replacement)
            if replacement == path or successor is None or successor["lifecycle"] == "deprecated":
                errors.append(f"{path}: replacement must be a retained, non-deprecated document: {replacement}")
    for document in documents.values():
        state = document["lifecycle"]
        if state not in STATES - {"removed"}:
            errors.append(f"{document['path']}: invalid lifecycle: {state}")
        declared = metadata((ROOT / document["path"]).read_text(encoding="utf-8")).get("lifecycle")
        if declared and document["path"] in records and declared != records[document["path"]]["lifecycle"]:
            errors.append(f"{document['path']}: conflicting lifecycle declarations")
        if state == "deprecated" and document["path"] not in records:
            errors.append(f"{document['path']}: deprecated document requires lifecycle ledger entry")
    return errors


def validate_changes(catalog, base):
    """Compare the chosen base to saved files, including untracked Markdown; never mutate Git."""
    revision = git("rev-parse", "--verify", "--end-of-options", base + "^{commit}").decode().strip()
    previous = set(git("ls-tree", "-r", "--name-only", "-z", revision).decode().split("\0"))
    current = {d["path"] for d in catalog["documents"]}
    records = lifecycle_records()
    errors = []
    for path in sorted(current - previous):
        meta = metadata((ROOT / path).read_text(encoding="utf-8"))
        for field in ("id", "kind", "authority", "status", "updated", "lifecycle"):
            if not meta.get(field):
                errors.append(f"{path}: new document requires metadata: {field}")
        if meta.get("lifecycle") not in {"current", "historical", "deprecated"}:
            errors.append(f"{path}: new document must declare current/historical/deprecated lifecycle")
    for path in sorted(previous - current):
        indexed = ("/" not in path or path.startswith("docs/") or (path.startswith("lean/") and path.count("/") == 1))
        if not indexed or not path.endswith(".md") or path == INDEX:
            continue
        entry = records.get(path, {})
        if entry.get("lifecycle") != "removed":
            errors.append(f"{path}: deletion requires removal ledger entry")
        elif hashlib.sha256(git("show", f"{revision}:{path}")).hexdigest() != entry["sha256"]:
            errors.append(f"{path}: removed content differs from base; preserve correct recovery snapshot")
    return errors


def collect():
    records = lifecycle_records()
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
        try:
            meta = metadata(source)
        except ValueError as error:
            raise ValueError(f"{relative}: {error}") from None
        sections = headings(source)
        category = meta.get("kind", kind(path))
        entry = records.get(relative, {})
        # Historical means a dated process/evidence artifact, never "completed" or "invalid".
        default_state = "historical" if category in {"plan", "report"} else "unclassified"
        state = entry.get("lifecycle", meta.get("lifecycle", default_state))
        documents.append({
            "path": relative, "id": meta.get("id", relative),
            "title": sections[0]["title"] if sections else path.stem,
            "kind": category,
            "lifecycle": state,
            "lifecycle_reason": entry.get("reason"),
            "replacements": entry.get("replacements", []),
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
    return {"schema_version": 2, "path_base": "repository-root",
            "notice": "Dates and mentioned_ids are retrieval hints, never approval or implementation status.",
            "topics": topics,
            "retired_documents": [entry for entry in records.values() if entry["lifecycle"] == "removed"],
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
        rows += [f"## {category}（{len(selected)}）", "", "| 文档 | 生命周期 | 路径 |", "|---|---|---|"]
        for document in selected:
            target = quote(relpath(ROOT / document["path"], (ROOT / INDEX).parent), safe="/-._")
            title = document["title"].replace("|", "\\|")
            rows.append(f"| [{title}]({target}) | {document['lifecycle']} | `{document['path']}` |")
        rows.append("")
    rows += ["## 已移除文档", "", "恢复版本、原因和替代入口见 [生命周期台账](../lifecycle.json)。",
             "用 `python3 scripts/docs.py inspect <旧路径>` 查询；已移除正文不参与全文搜索。", ""]
    for entry in catalog["retired_documents"]:
        rows.append(f"- `{entry['path']}`：{entry['reason']}")
    return {CATALOG: json.dumps(catalog, ensure_ascii=False, indent=2) + "\n",
            INDEX: "\n".join(rows)}


def validate(catalog):
    errors, ids = validate_lifecycle(catalog), set()
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
            for field in ("id", "kind", "authority", "status", "lifecycle", "updated"):
                if not meta.get(field):
                    errors.append(f"{document['path']}: missing metadata field: {field}")
            if not re.fullmatch(r"\d{4}-\d{2}-\d{2}", meta.get("updated", "")):
                errors.append(f"{document['path']}: updated must be YYYY-MM-DD")
            else:
                try:
                    date.fromisoformat(meta["updated"])
                except ValueError:
                    errors.append(f"{document['path']}: invalid calendar date")
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
                elif field == "guide" and documents[destination.relative_to(ROOT).as_posix()]["lifecycle"] == "deprecated":
                    errors.append(f"topic {topic['id']}: deprecated guide: {value}")
    return errors


def search(catalog, query, limit, category, lifecycle=None):
    terms = query.casefold().split()
    results = []
    for document in catalog["documents"]:
        if lifecycle == "all":
            pass
        elif lifecycle and document["lifecycle"] != lifecycle:
            continue
        elif not lifecycle and document["lifecycle"] == "deprecated":
            continue
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
                        "status": document["status"], "lifecycle": document["lifecycle"],
                        "lifecycle_reason": document["lifecycle_reason"], "replacements": document["replacements"],
                        "topic_ids": document["topic_ids"],
                        "score": score, "matches": matches})
    return sorted(results, key=lambda d: (-d["score"], d["path"]))[:limit]


def inspect_document(catalog, path):
    entry = next((d for d in catalog["documents"] + catalog["retired_documents"] if d["path"] == path), None)
    if entry is None:
        raise ValueError(f"unknown documentation path: {path}")
    incoming = []
    for document in catalog["documents"]:
        for link in document["links"]:
            destination, _ = resolve(ROOT / document["path"], link["target"])
            if destination == (ROOT / path).resolve():
                incoming.append({"path": document["path"], "line": link["line"], "target": link["target"]})
    return {"document": entry, "incoming_links": incoming,
            "topic_ids": [t["id"] for t in catalog["topics"] if path == t["guide"] or path in t["sources"]]}


def self_test():
    global ROOT
    original = ROOT
    try:
        with tempfile.TemporaryDirectory() as temp:
            ROOT = Path(temp)
            (ROOT / "docs/architecture").mkdir(parents=True)
            (ROOT / TOPICS).write_text("[]", encoding="utf-8")
            (ROOT / LIFECYCLE).write_text('{"schema_version": 1, "entries": []}', encoding="utf-8")
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
            # A small real Git fixture exercises recovery, deletion guards and new-file policy.
            git("init", "-q")
            git("add", ".")
            git("-c", "user.name=Docs test", "-c", "user.email=docs@example.invalid",
                "-c", "core.hooksPath=/dev/null", "-c", "commit.gpgSign=false", "commit", "-qm", "fixture")
            revision = git("rev-parse", "HEAD").decode().strip()
            old = ROOT / "docs/中文.md"
            raw = old.read_bytes()
            old.unlink()
            assert any("deletion requires" in e for e in validate_changes(collect(), revision))
            entry = {"path": "docs/中文.md", "lifecycle": "removed", "id": "shared",
                     "updated": "2026-10-24", "reason": "replaced duplicate explanation",
                     "replacements": ["README.md"], "revision": revision,
                     "sha256": hashlib.sha256(raw).hexdigest()}
            def ledger(entries):
                (ROOT / LIFECYCLE).write_text(json.dumps({"schema_version": 1, "entries": entries}), encoding="utf-8")
            ledger([entry])
            assert not validate_changes(collect(), revision)
            assert any("retired id is reused" in e for e in validate_lifecycle(collect()))
            (ROOT / "docs/other.md").write_text("# Other\nCOW fork", encoding="utf-8")
            assert not validate_lifecycle(collect()), validate_lifecycle(collect())
            assert inspect_document(collect(), "docs/中文.md")["document"]["revision"] == revision
            entry["sha256"] = "0" * 64
            ledger([entry])
            assert any("recovery sha256 mismatch" in e for e in validate_lifecycle(collect()))
            assert any("differs from base" in e for e in validate_changes(collect(), revision))
            entry["sha256"] = hashlib.sha256(raw).hexdigest()
            entry["revision"] = "0" * 40
            ledger([entry])
            assert any("cannot recover" in e for e in validate_lifecycle(collect()))
            entry["revision"] = revision
            ledger([entry])
            old.write_bytes(raw)
            assert any("removed path must not exist" in e for e in validate_lifecycle(collect()))
            old.unlink()
            retired = {"path": "docs/other.md", "lifecycle": "deprecated", "updated": "2026-10-24",
                       "reason": "old instructions", "replacements": ["README.md"]}
            ledger([entry, retired])
            assert not search(collect(), "COW fork", 10, None)
            assert search(collect(), "COW fork", 10, None, "all")[0]["lifecycle"] == "deprecated"
            assert search(collect(), "COW fork", 10, None, "deprecated")
            assert not search(collect(), "COW fork", 10, None, "current")
            retired["replacements"] = ["docs/other.md"]
            ledger([entry, retired])
            assert any("replacement must" in e for e in validate_lifecycle(collect()))
            retired["replacements"] = ["README.md"]
            ledger([entry, retired])
            (ROOT / "docs/new.md").write_text("# New\n", encoding="utf-8")
            assert any("new document requires" in e for e in validate_changes(collect(), revision))
            (ROOT / "docs/new.md").write_text(
                "---\nid: new-guide\nkind: guide\nauthority: explanatory\nstatus: maintained\n"
                "lifecycle: current\nupdated: 2026-10-24\n---\n# New\n", encoding="utf-8")
            assert not validate_changes(collect(), revision), validate_changes(collect(), revision)
            retired["replacements"] = ["missing.md"]
            ledger([entry, retired])
            assert any("replacement must" in e for e in validate_lifecycle(collect()))
            for invalid in [dict(retired, path="../outside.md"), dict(retired, lifecycle="done"),
                            dict(retired, lifecycle=[]), dict(retired, reason=""),
                            dict(retired, updated="2026-99-24")]:
                ledger([invalid])
                try:
                    collect()
                except ValueError:
                    pass
                else:
                    raise AssertionError("invalid lifecycle ledger must fail")
            ledger([entry])
            for malformed in ["---\nid: x\nid: y\n---\n# X", "---\nid: x\n# X"]:
                try:
                    metadata(malformed)
                except ValueError:
                    pass
                else:
                    raise AssertionError("malformed metadata must fail")
    finally:
        ROOT = original
    print("PASS docs self-test: Markdown, metadata, search, determinism, lifecycle, Git recovery, change guards")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    for command in ("build", "check", "self-test"):
        subparser = commands.add_parser(command)
        if command == "check":
            subparser.add_argument("--base", help="Git base: also validate new document metadata and removal records")
    search_parser = commands.add_parser("search")
    search_parser.add_argument("query")
    search_parser.add_argument("--limit", type=int, default=8)
    search_parser.add_argument("--kind")
    search_parser.add_argument("--json", action="store_true")
    search_parser.add_argument("--lifecycle", choices=sorted(STATES - {"removed"}) + ["all"])
    inspect_parser = commands.add_parser("inspect")
    inspect_parser.add_argument("path")
    topic_parser = commands.add_parser("topic")
    topic_parser.add_argument("id")
    args = parser.parse_args()
    if args.command == "self-test":
        self_test()
        return 0
    catalog = collect()
    if args.command == "inspect":
        print(json.dumps(inspect_document(catalog, args.path), ensure_ascii=False, indent=2))
        return 0
    if args.command == "search":
        if not 1 <= args.limit <= 100:
            parser.error("--limit must be between 1 and 100")
        if not args.query.strip():
            parser.error("query must not be empty")
        results = search(catalog, args.query, args.limit, args.kind, args.lifecycle)
        if args.json:
            print(json.dumps(results, ensure_ascii=False, indent=2))
        else:
            for result in results:
                topics = ",".join(result["topic_ids"]) or "unassigned"
                print(f"{result['path']} [{result['kind']}; {result['authority']}; lifecycle={result['lifecycle']}; topics={topics}] {result['title']}")
                for match in result["matches"]:
                    print(f"  L{match['line']}: {match['text']}")
        return 0
    if args.command == "topic":
        topic = next((t for t in catalog["topics"] if t["id"] == args.id), None)
        if topic is None:
            parser.error("unknown topic; choose: " + ", ".join(t["id"] for t in catalog["topics"]))
        documents = {d["path"]: d for d in catalog["documents"]}
        states = [{field: documents[path][field] for field in ("path", "status", "lifecycle", "replacements")}
                  for path in [topic["guide"], *topic["sources"]]]
        print(json.dumps({**topic, "source_lifecycle": states}, ensure_ascii=False, indent=2))
        return 0
    if args.command == "build":
        for name, content in products(catalog).items():
            (ROOT / name).write_text(content, encoding="utf-8")
        print(f"Built {len(catalog['documents'])} documents, {sum(len(d['sections']) for d in catalog['documents'])} sections")
        return 0
    errors = validate(catalog)
    if args.base:
        errors += validate_changes(catalog, args.base)
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
