#!/bin/bash
set -e

# w3mのバイナリパスを取得（whichで探す）
W3M_BIN_PATH=$(which w3m)
if [ -z "$W3M_BIN_PATH" ]; then
    echo "Error: w3mがシステムにインストールされていません。sudo apt install w3m などでインストールしてください。"
    exit 1
fi

# lib/binディレクトリを作成し、w3mバイナリをコピー
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
DEST_DIR="$SCRIPT_DIR/../lib/bin"
mkdir -p "$DEST_DIR"
cp "$W3M_BIN_PATH" "$DEST_DIR/"

echo "w3mバイナリを$DEST_DIRにコピーしました。"

