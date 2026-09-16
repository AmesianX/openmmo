# 지형 파일의 nginx 직접 제공

지형은 기존 HTTP URL과 MessagePack 래퍼를 사용한다. 프로토콜 82부터 풀 payload는 GR04 셀별 개수다. 웹은 `full`, 에이전트는 높이·지면 재질만 포함하는 `ground`를 받는다. 게임 서버는 WebSocket으로 버전을 알리고, nginx는 준비된 파일을 읽어 제공한다.

## 파일 준비

```bash
cargo run --release -p onlinerpg-terrain --bin terrain-snapshots -- data/terrain
```

준비 명령은 기존 V3 풀잎 목록도 V4 셀별 개수로 읽어 전송 본문에 담는다. 디스크의 원본까지 줄이려면 서버를 중지하고 [별도 변환 명령](VEGETATION_SYSTEM.md#기존-파일-변환)을 실행한다.

`TERRAIN_DIR/snapshots/` 아래에 다음 파일을 만든다.

```text
full/<x>/<z>/<sha256>       웹용 MessagePack 본문
ground/<x>/<z>/<sha256>     에이전트용 MessagePack 본문
index/<x>/<z>.json         두 해시·원본 파일 상태·본문 크기
```

처음에는 원본 타일 전체의 전송용 파일을 만든다. 이후에는 원본 파일의 크기·수정 시각, 전송 형식 버전, 생성된 파일의 존재·크기를 확인해 변경·누락된 타일만 준비한다. 게임 서버는 시작할 때 전체 지형을 순회하지 않는다. 첫 구독 시 해당 타일의 기존 인덱스와 원본 상태를 검증하고 메모리에 적재한다. 사전 생성된 파일이 없으면 그 타일만 준비한다. 실행 중 건축·조경 등의 변경은 해당 타일만 다시 준비한다.

새 본문 파일을 원자적으로 저장한 다음 인덱스를 교체하고 WebSocket 버전을 게시한다. 원본 파일 읽기나 출력 저장이 실패하면 새 버전을 게시하지 않는다. 전송 시에는 직렬화·해시 계산을 반복하지 않는다.

전송 파일은 원본과 별도이므로 최초 준비에 디스크 공간과 시간이 필요하다. 구버전 파일은 진행 중인 다운로드와 이전 클라이언트 캐시를 위해 보존하며 자동 삭제하지 않는다. 기존 인덱스가 가리키는 파일은 삭제하지 않는다. 전송 형식을 변경하면 `terrain/src/snapshot.rs`의 `SNAPSHOT_FORMAT`을 올린다.

프로세스 밖에서 원본을 바꾼 뒤에는 서버를 재시작한다. 원본 크기·수정 시각을 모두 보존하며 내용을 바꾸는 작업은 해당 `index/<x>/<z>.json`도 제거해 재생성해야 한다. 생성 도구와 서버를 동시에 실행할 경우 변경 중인 파일을 감지하면 준비 명령이 실패할 수 있으므로 변경이 끝난 후 다시 실행한다.

## Docker Compose

서버 이미지에 `terrain-snapshots` 바이너리를 포함한다. `terrain-init`이 베이크 후 스냅샷을 준비하고, 기존 terrain 볼륨을 사용하는 경우에도 변경 여부를 확인한다. client는 같은 볼륨을 `/terrain:ro`로 마운트한다. `docker/nginx.conf.template`의 스냅샷 location이 `/terrain/snapshots/` 파일을 직접 제공한다.

## systemd 운영 서버

`tools/deploy-prod.sh`가 스냅샷 도구를 빌드하고 `${TERRAIN_DIR:-$REPO/data/terrain}`의 파일을 준비한다. 이 단계가 실패하면 웹 파일 게시·서비스 재시작을 진행하지 않는다. 서버가 실제 사용하는 `--terrain-dir`와 같은 디렉터리를 지정해야 한다. 스냅샷은 웹 번들의 `rsync --delete` 대상인 `WEBROOT` 안에 두지 않는다.

운영 nginx는 수동 관리하므로 `/etc/nginx/sites-available/openmmo`의 해당 `server` 블록에 아래 두 location을 반영한다. 경로는 운영 서버의 절대 경로로 바꾼다. 기존에 스냅샷용 `^~` 프록시 location이 있으면 제거해 정규식 location이 적용되게 한다.

```nginx
location ~ "^/api/terrain/snapshot/(full|ground)/(-?[0-9]+)/(-?[0-9]+)/([0-9a-f]{64})$" {
    alias /ABSOLUTE/TERRAIN_DIR/snapshots/$1/$2/$3/$4;
    default_type application/octet-stream;
    sendfile on;
    add_header Cache-Control "public, max-age=31536000, immutable";
    error_page 404 = @terrain_snapshot_missing;
}

location @terrain_snapshot_missing {
    add_header Cache-Control "no-store" always;
    return 404;
}
```

nginx worker가 상위 디렉터리를 탐색하고 스냅샷 파일을 읽을 수 있어야 한다. 원본·스냅샷의 쓰기 권한은 게임 서버 계정에 유지한다. `nginx -t`를 통과한 뒤 reload한다. 생성 파일은 프로토콜 변경 없이 기존 URL로 제공되므로 배포 순서는 **스냅샷 준비 → nginx 경로·권한 확인 → 서버 시작**이다.

## HTTP와 검증

- 생성된 파일은 `200`, `Cache-Control: public, max-age=31536000, immutable`로 제공한다. nginx의 정적 파일 ETag로 재검증하면 `304`다. URL의 SHA-256과 nginx의 ETag는 서로 다른 식별자여도 된다.
- 현재 원본과 달라도 이전 해시의 파일은 이전 바이트를 그대로 제공한다. 파일이 없으면 `404, no-store`다. 웹·에이전트는 최신 버전을 재동기화한다.
- nginx 없이 실행하는 개발 환경의 Rust HTTP 경로도 같은 파일을 제공한다. 이 경로는 요청 때 원본을 읽거나 직렬화·해시 계산을 하지 않는다.

```bash
bash tools/check-nginx-conf.sh
bash tools/test-terrain-static.sh
bash tools/test-deploy-prod.sh
```

정적 서빙 테스트는 임시 타일을 준비하고 실제 nginx를 Unix 소켓에서 실행한다. 게임 백엔드 없이 본문 일치·304·404의 캐시 정책·변경 전 파일 보존을 검증한다. 운영 서비스나 운영 nginx 설정을 변경하지 않는다.

Rust의 `runtime_lookup_prepares_only_the_requested_tile` 테스트는 다른 타일의 원본이 읽기 불가능해도 요청한 타일만 준비하는 동작을 확인한다.
