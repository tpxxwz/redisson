#!/usr/bin/env python3
"""
分析 org/redisson 目录下未被任何其他类引用的 Java 类。
引用范围仅限于同一目录树内的 .java 文件。
"""

import os
import re
import sys
from pathlib import Path
from collections import defaultdict

ROOT = Path(__file__).parent  # org/redisson 目录

def collect_java_files():
    return list(ROOT.rglob("*.java"))

def extract_class_name(filepath: Path) -> str:
    """从文件名提取类名（Java 规范：文件名 == public 类名）"""
    return filepath.stem

def extract_declared_names(content: str) -> list[str]:
    """提取文件中声明的所有顶层类/接口/枚举/注解名称"""
    pattern = re.compile(
        r'(?:^|\s)(?:public|protected|private|abstract|final|sealed|non-sealed)?\s*'
        r'(?:public|protected|private|abstract|final|sealed|non-sealed)?\s*'
        r'(?:class|interface|enum|@interface|record)\s+(\w+)',
        re.MULTILINE
    )
    return pattern.findall(content)

def build_reference_index(files: list[Path]) -> dict[str, list[Path]]:
    """
    构建：类名 -> [引用该类名的文件列表]
    使用单词边界匹配，避免 Lock 匹配到 RedissonLock 这类情况。
    """
    # 先收集所有文件内容
    file_contents: dict[Path, str] = {}
    for f in files:
        try:
            file_contents[f] = f.read_text(encoding="utf-8")
        except Exception as e:
            print(f"[WARN] 读取失败: {f}: {e}", file=sys.stderr)

    # 收集所有类名及其所在文件
    class_to_file: dict[str, Path] = {}
    file_to_classes: dict[Path, list[str]] = {}
    for f, content in file_contents.items():
        names = extract_declared_names(content)
        # 主类名以文件名为准
        stem = f.stem
        all_names = list(dict.fromkeys([stem] + names))  # 去重，保持顺序
        file_to_classes[f] = all_names
        for name in all_names:
            if name not in class_to_file:
                class_to_file[name] = f

    # 统计每个类被哪些文件引用（排除自身）
    references: dict[str, list[Path]] = defaultdict(list)
    for class_name in class_to_file:
        pattern = re.compile(r'\b' + re.escape(class_name) + r'\b')
        owner_file = class_to_file[class_name]
        for f, content in file_contents.items():
            if f == owner_file:
                continue
            if pattern.search(content):
                references[class_name].append(f)

    return class_to_file, file_to_classes, references

def relative(p: Path) -> str:
    try:
        return str(p.relative_to(ROOT))
    except ValueError:
        return str(p)

def main():
    files = collect_java_files()
    print(f"扫描文件数: {len(files)}\n")

    class_to_file, file_to_classes, references = build_reference_index(files)

    # 找出主类（以文件名为准）没有任何引用的文件
    unused_files = []
    for f in sorted(files, key=lambda p: str(p)):
        stem = f.stem
        if not references.get(stem):
            unused_files.append(f)

    print(f"{'=' * 60}")
    print(f"未被引用的类（共 {len(unused_files)} 个）:")
    print(f"{'=' * 60}")
    for f in unused_files:
        print(f"  {relative(f)}")

    print()
    print(f"{'=' * 60}")
    print(f"被引用的类（共 {len(files) - len(unused_files)} 个）:")
    print(f"{'=' * 60}")
    for f in sorted(files, key=lambda p: str(p)):
        stem = f.stem
        refs = references.get(stem, [])
        if refs:
            print(f"  {relative(f)}  <- 被 {len(refs)} 个文件引用")

if __name__ == "__main__":
    main()
