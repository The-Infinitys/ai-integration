#!/bin/bash
set -e

# スクリプトのディレクトリからの相対パスでlib/w3m/に移動
cd "$(dirname "$0")/../lib/w3m/"

# configureとmake
if [ ! -f configure ]; then
    echo "Error: configureスクリプトが見つかりません。w3mのソースが正しく配置されているか確認してください。"
    exit 1
fi

./configure
make

# バイナリをlib/binに移動
mkdir -p ../../bin
cp w3m ../../bin/

echo "w3mのビルドと配置が完了しました。"
