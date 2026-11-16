#!/usr/bin/env python3
# 一次性迁移:blocks 表补 images 列,版本抬到 19。用前先关掉 mypi。
# 用法: python3 migrate-images.py <history.db 的路径>
import shutil, sqlite3, sys

db = sys.argv[1]
shutil.copy2(db, db + ".bak-v18")  # 先备份,出事可回退
conn = sqlite3.connect(db)
conn.execute("PRAGMA wal_checkpoint(TRUNCATE);")
cols = [r[1] for r in conn.execute("PRAGMA table_info(blocks);")]
if "images" not in cols:
    conn.execute("ALTER TABLE blocks ADD COLUMN images TEXT;")
conn.execute("PRAGMA user_version = 19;")
conn.commit()
conn.close()
print("done:", db)
