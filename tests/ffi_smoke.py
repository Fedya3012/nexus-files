"""FFI smoke test: drives the compiled core through generated Python bindings.

Usage: python tests/ffi_smoke.py <dir containing nexus_core.py and the cdylib>
"""
import os
import sys
import tempfile
import time

sys.path.insert(0, sys.argv[1])
import nexus_core as n  # noqa: E402

d = tempfile.mkdtemp()
for name in ["Báo cáo quý 3.xlsx", "Đề xuất dự án.pdf", "notes.md"]:
    with open(os.path.join(d, name), "w", encoding="utf-8") as f:
        f.write("xin chào")
os.mkdir(os.path.join(d, "Thiết kế UI"))

sort = n.SortOptions(key=n.SortKey.NAME, descending=False, folders_first=True)
listing = n.list_directory(d, n.ListOptions(show_hidden=False, sort=sort))
assert [e.name for e in listing.entries][0] == "Thiết kế UI", listing.entries

core = n.NexusCore(os.path.join(d, ".data"))
hits = core.search(d, "bao cao ext:xlsx", 10, n.CancelToken())
assert [h.name for h in hits.files] == ["Báo cáo quý 3.xlsx"], hits

assert core.omnibar("> doi ten", d, 3).commands, "palette should find rename"

job = core.enqueue(n.FileOperation.COPY(sources=[os.path.join(d, "notes.md")], destination=d,
                                        conflict=n.ConflictPolicy.KEEP_BOTH))
while core.job(job).state in (n.JobState.QUEUED, n.JobState.RUNNING):
    time.sleep(0.01)
assert core.job(job).state == n.JobState.COMPLETED
assert os.path.exists(os.path.join(d, "notes (2).md"))
assert core.undo_title()

try:
    core.execute(n.FileOperation.RENAME(path=os.path.join(d, "notes.md"), new_name="bad/name"))
    raise AssertionError("invalid name accepted")
except n.NexusError.Invalid:
    pass

assert n.preview(os.path.join(d, "notes.md")).content.is_TEXT()
print("FFI smoke test OK, core", n.core_version())
