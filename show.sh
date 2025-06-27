#!/bin/bash

# ソースディレクトリ
SRC_DIR="src"

# ヘルプメッセージ
usage() {
  echo "Usage: show.sh --help"
  echo "       show.sh --dir <directory>"
}

# 引数の処理
while getopts ":h::d:" opt; do
  case "$opt" in
    h)
      usage
      exit 0
      ;;
    d)
      SRC_DIR="$OPTARG"
      ;;
    *)
      echo "Invalid option: $opt" >&2
      usage
      exit 1
      ;;
  esac
done

# srcディレクトリが存在するか確認
if [ ! -d "$SRC_DIR" ]; then
    echo "Error: $SRC_DIR directory not found" >&2
    exit 1
fi

# srcディレクトリ内のすべての.rsファイルを処理
find "$SRC_DIR" -type f -name "*.rs" | while read -r file; do
    # ファイルパスの出力
    echo "===========$file"
    # ファイル内容の出力
    cat "$file"
    # ファイル終了の区切り
    echo "==========="
done