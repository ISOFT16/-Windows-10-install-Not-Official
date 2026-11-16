import os
import tempfile
import time
import unittest

from microos.boot import boot
from microos.fs import VFS, FSError
from microos.memory import MemoryManager, OutOfMemory


class Base(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.disk = os.path.join(self.tmp.name, "disk.json")
        self.k, self.sh = boot(self.disk)

    def tearDown(self):
        self.k.shutdown()
        self.tmp.cleanup()

    def run_cmd(self, line):
        return self.sh.execute(line)


class TestFS(Base):
    def test_files_and_dirs(self):
        self.assertEqual(self.run_cmd("pwd"), "/home/user")
        self.run_cmd("mkdir -p a/b/c")
        self.run_cmd("echo привет > a/b/c/f.txt")
        self.run_cmd("echo мир >> a/b/c/f.txt")
        self.assertEqual(self.k.fs.read("/home/user/a/b/c/f.txt"), "привет\nмир\n")
        self.run_cmd("cd a/b")
        self.assertEqual(self.run_cmd("pwd"), "/home/user/a/b")
        self.run_cmd("cd ..")
        self.run_cmd("cp b b2")
        self.assertIn("f.txt", self.run_cmd("ls b2/c"))
        self.run_cmd("mv b2 b3")
        self.assertIn("b3/", self.run_cmd("ls"))
        self.assertIn("не пуст", self.run_cmd("rm b3"))
        self.run_cmd("rm -r b3")
        self.assertNotIn("b3", self.run_cmd("ls"))

    def test_persistence(self):
        self.run_cmd("echo сохранено > note.txt")
        self.k.shutdown()
        k2, sh2 = boot(self.disk)
        try:
            self.assertEqual(sh2.execute("cat note.txt"), "сохранено")
        finally:
            k2.shutdown()

    def test_permissions(self):
        self.assertIn("отказано", self.run_cmd("echo x > /etc/passwd"))
        self.assertIn("отказано", self.run_cmd("rm /etc/hostname"))
        self.run_cmd("echo ok > /tmp/t")
        self.assertEqual(self.run_cmd("cat /tmp/t"), "ok")

    def test_vfs_paths(self):
        self.assertEqual(VFS.resolve("../x", "/a/b"), "/a/x")
        self.assertEqual(VFS.resolve("/../..", "/"), "/")
        fs = VFS()
        fs.mkdir("/d")
        with self.assertRaises(FSError):
            fs.move("/d", "/d/inner")


class TestShell(Base):
    def test_tokenize(self):
        from microos.shell import Shell
        self.assertEqual(Shell._tokenize('echo a>f;cat "x y"|wc'),
                         ["echo", "a", ">", "f", ";", "cat", "x y", "|", "wc"])

    def test_pipes(self):
        self.run_cmd("echo b > f; echo a >> f; echo b >> f")
        self.assertEqual(self.run_cmd("cat f | sort | uniq"), "a\nb")
        self.assertEqual(self.run_cmd("cat f | grep b | wc"), "2 2 3")

    def test_vars_and_calc(self):
        self.run_cmd("export X=42")
        self.assertEqual(self.run_cmd("echo $X"), "42")
        self.assertEqual(self.run_cmd("calc 2+2*sqrt(16)"), "10")
        self.assertIn("деление на ноль", self.run_cmd("calc 1/0"))
        self.assertIn("недопустимое", self.run_cmd("calc __import__('os')"))

    def test_unknown_and_history(self):
        self.assertIn("не найдена", self.run_cmd("abracadabra"))
        self.run_cmd("echo hi")
        self.assertEqual(self.run_cmd("!2"), "hi")

    def test_editor(self):
        self.run_cmd("edit doc.txt")
        self.run_cmd("строка 1")
        self.run_cmd("строка 2")
        self.run_cmd(":d 1")
        self.assertIn("сохранено", self.run_cmd(":w"))
        self.assertEqual(self.k.fs.read("/home/user/doc.txt"), "строка 2\n")

    def test_su(self):
        self.run_cmd("su")
        self.assertTrue(self.sh.secret)
        self.assertIn("неверный", self.run_cmd("wrong"))
        self.run_cmd("su")
        self.run_cmd("root")
        self.assertEqual(self.run_cmd("whoami"), "root")
        self.run_cmd("useradd anna")
        self.run_cmd("pw")
        self.assertIn("anna", self.run_cmd("users"))
        self.run_cmd("exit")
        self.assertEqual(self.run_cmd("whoami"), "user")

    def test_python_script(self):
        out = self.run_cmd("py hello.py")
        self.assertIn("Привет из MicroOS!", out)
        self.assertIn("5 * 5 = 25", out)
        self.run_cmd("echo 'import os' > bad.py")
        self.assertIn("ImportError", self.run_cmd("py bad.py"))

    def test_completion(self):
        self.assertEqual(self.sh.complete("neof"), "neofetch ")
        self.assertEqual(self.sh.complete("cat hel"), "cat hello.py")


class TestKernel(Base):
    def test_processes(self):
        out = self.run_cmd("run counter 3")
        pid = int(out.split("]")[0][1:])
        self.assertIn("counter", self.run_cmd("ps"))
        deadline = time.time() + 6
        while self.k.procs[pid].alive and time.time() < deadline:
            time.sleep(0.1)
        self.assertIn("готово", self.run_cmd(f"out {pid}"))
        self.assertEqual(self.k.mem.usage_of(pid), 0)

    def test_kill(self):
        root_pid = self.k.spawn("spinner", user="root")
        self.assertIn("отказано", self.run_cmd(f"kill {root_pid}"))
        pid = self.k.spawn("spinner", user="user")
        time.sleep(0.2)
        self.assertGreater(self.k.procs[pid].cpu_ticks, 0)
        self.assertIn("завершён", self.run_cmd(f"kill {pid}"))
        self.assertFalse(self.k.procs[pid].alive)

    def test_memory(self):
        m = MemoryManager(8 * 4096)
        m.alloc(5, 4096 * 3)
        self.assertEqual(m.used_pages(), 3)
        with self.assertRaises(OutOfMemory):
            m.alloc(6, 4096 * 6)
        self.assertEqual(m.free_pid(5), 3)


if __name__ == "__main__":
    unittest.main()
