"""Виртуальная файловая система MicroOS.

Всё дерево хранится в памяти и сохраняется на "диск" — JSON-файл образа.
"""

from __future__ import annotations

import json
import os
import posixpath
import time


class FSError(Exception):
    pass


def _dir(owner="root"):
    return {"type": "dir", "owner": owner, "mtime": time.time(), "children": {}}


def _file(data="", owner="root"):
    return {"type": "file", "owner": owner, "mtime": time.time(), "data": data}


class VFS:
    def __init__(self, image_path: str | None = None):
        self.image_path = image_path
        self.root = _dir()
        if image_path and os.path.exists(image_path):
            try:
                with open(image_path, encoding="utf-8") as f:
                    self.root = json.load(f)
            except (OSError, ValueError):
                self.root = _dir()  # образ повреждён — начинаем с чистого диска

    # ---------- диск ----------
    def sync(self):
        if not self.image_path:
            return
        d = os.path.dirname(self.image_path)
        if d:
            os.makedirs(d, exist_ok=True)
        tmp = self.image_path + ".tmp"
        with open(tmp, "w", encoding="utf-8") as f:
            json.dump(self.root, f, ensure_ascii=False)
        os.replace(tmp, self.image_path)

    def format(self):
        self.root = _dir()

    # ---------- пути ----------
    @staticmethod
    def resolve(path: str, cwd: str = "/") -> str:
        if not path:
            return cwd
        if not path.startswith("/"):
            path = posixpath.join(cwd, path)
        norm = posixpath.normpath(path)
        if norm.startswith("//"):
            norm = "/" + norm.lstrip("/")
        return norm

    @staticmethod
    def _parts(path: str) -> list[str]:
        return [p for p in path.split("/") if p]

    def _node(self, path: str):
        node = self.root
        for part in self._parts(path):
            if node["type"] != "dir" or part not in node["children"]:
                return None
            node = node["children"][part]
        return node

    def _parent(self, path: str):
        parts = self._parts(path)
        if not parts:
            raise FSError("нельзя изменить корневой каталог")
        parent = self._node("/" + "/".join(parts[:-1]))
        if parent is None or parent["type"] != "dir":
            raise FSError(f"нет такого каталога: {posixpath.dirname(path)}")
        return parent, parts[-1]

    @staticmethod
    def _check_write(node, user):
        if user != "root" and node.get("owner") not in (user, "*", None):
            raise FSError("отказано в доступе")

    # ---------- запросы ----------
    def exists(self, path):
        return self._node(path) is not None

    def isdir(self, path):
        n = self._node(path)
        return n is not None and n["type"] == "dir"

    def isfile(self, path):
        n = self._node(path)
        return n is not None and n["type"] == "file"

    def stat(self, path):
        n = self._node(path)
        if n is None:
            raise FSError(f"нет такого файла или каталога: {path}")
        size = len(n["data"].encode("utf-8")) if n["type"] == "file" else len(n["children"])
        return {"type": n["type"], "owner": n.get("owner", "root"),
                "mtime": n.get("mtime", 0), "size": size}

    def listdir(self, path):
        n = self._node(path)
        if n is None:
            raise FSError(f"нет такого каталога: {path}")
        if n["type"] != "dir":
            raise FSError(f"это не каталог: {path}")
        return sorted(n["children"])

    def read(self, path):
        n = self._node(path)
        if n is None:
            raise FSError(f"нет такого файла: {path}")
        if n["type"] != "file":
            raise FSError(f"это каталог: {path}")
        return n["data"]

    # ---------- изменения ----------
    def mkdir(self, path, user="root", parents=False):
        if self.exists(path):
            if parents and self.isdir(path):
                return
            raise FSError(f"уже существует: {path}")
        if parents:
            cur = "/"
            for part in self._parts(path):
                cur = posixpath.join(cur, part)
                if not self.exists(cur):
                    self.mkdir(cur, user)
                elif not self.isdir(cur):
                    raise FSError(f"это не каталог: {cur}")
            return
        parent, name = self._parent(path)
        self._check_write(parent, user)
        parent["children"][name] = _dir(user)

    def write(self, path, data, user="root", append=False):
        n = self._node(path)
        if n is not None:
            if n["type"] != "file":
                raise FSError(f"это каталог: {path}")
            self._check_write(n, user)
            n["data"] = (n["data"] + data) if append else data
            n["mtime"] = time.time()
            return
        parent, name = self._parent(path)
        self._check_write(parent, user)
        parent["children"][name] = _file(data, user)

    def touch(self, path, user="root"):
        n = self._node(path)
        if n is not None:
            n["mtime"] = time.time()
        else:
            self.write(path, "", user)

    def remove(self, path, user="root", recursive=False):
        n = self._node(path)
        if n is None:
            raise FSError(f"нет такого файла или каталога: {path}")
        if n["type"] == "dir" and n["children"] and not recursive:
            raise FSError(f"каталог не пуст: {path} (используйте rm -r)")
        self._check_write(n, user)
        parent, name = self._parent(path)
        del parent["children"][name]

    def copy(self, src, dst, user="root"):
        n = self._node(src)
        if n is None:
            raise FSError(f"нет такого файла: {src}")
        if self.isdir(dst):
            dst = posixpath.join(dst, posixpath.basename(src))
        if n["type"] == "dir" and (dst + "/").startswith(src.rstrip("/") + "/"):
            raise FSError("нельзя скопировать каталог внутрь себя")
        parent, name = self._parent(dst)
        self._check_write(parent, user)
        clone = json.loads(json.dumps(n))
        clone["owner"] = user
        parent["children"][name] = clone

    def move(self, src, dst, user="root"):
        n = self._node(src)
        if n is None:
            raise FSError(f"нет такого файла: {src}")
        self._check_write(n, user)
        if self.isdir(dst):
            dst = posixpath.join(dst, posixpath.basename(src))
        if (dst + "/").startswith(src.rstrip("/") + "/"):
            raise FSError("нельзя переместить каталог внутрь себя")
        parent, name = self._parent(dst)
        sp, sname = self._parent(src)
        parent["children"][name] = sp["children"].pop(sname)

    # ---------- обход ----------
    def walk(self, path):
        """Генератор (путь, узел) по всему поддереву."""
        n = self._node(path)
        if n is None:
            return
        stack = [(path, n)]
        while stack:
            p, node = stack.pop()
            yield p, node
            if node["type"] == "dir":
                for name in sorted(node["children"], reverse=True):
                    stack.append((posixpath.join(p, name), node["children"][name]))

    def usage(self):
        files = dirs = size = 0
        for _, node in self.walk("/"):
            if node["type"] == "dir":
                dirs += 1
            else:
                files += 1
                size += len(node["data"].encode("utf-8"))
        return files, dirs, size
