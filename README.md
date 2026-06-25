# fgc — Fast Git Cloner

대규모 Git 저장소의 클론을 지능적으로 최적화하는 CLI 도구입니다.

## 설치

```bash
# 소스에서 빌드
cargo install --path .

# 또는 릴리즈 바이너리 (GitHub Releases)
# https://github.com/Nergis0318/fgc/releases
```

## 사용법

```bash
# 자동 전략 (기본)
fgc clone https://github.com/torvalds/linux.git

# TUI 대시보드 모드
fgc clone <url> --tui

# sparse / shallow / blobless
fgc clone <url> --strategy sparse --paths src,docs
fgc clone <url> --strategy shallow --depth 1

# 참조 저장소 + LFS aria2c 가속
fgc clone <url> --reference ~/.cache/fgc/mirror
fgc clone <url> --lfs-backend aria2c --aria2c-connections 16

# 전략 벤치마크
fgc benchmark <url> --strategies shallow,blobless,full
```

## 설정 (`~/.config/fgc/config.toml`)

```toml
default_strategy = "auto"
lfs_jobs = 8
lfs_backend = "auto"      # auto | git | aria2c
aria2c_connections = 16
depth = 1
reference = "~/.cache/fgc/main"
tui = false
```

## 주요 기능

| 기능 | 설명 |
|------|------|
| 자동 전략 | GitHub 메타데이터 + CI 환경 감지 |
| TUI (`--tui`) | ratatui 기반 6단계 진행 대시보드 |
| aria2c LFS | 다중 연결 LFS 다운로드 (auto/git/aria2c) |
| Resume | 중단된 클론 이어하기 |
| Fallback | partial clone 미지원 시 shallow → full |
| Benchmark | 전략별 시간·용량 비교 |

## 요구 사항

- Git 2.23+
- git-lfs (LFS 저장소)
- aria2c (LFS 가속, 선택)
- Rust 1.70+ (빌드 시)

## 라이선스

Dual-licensed under [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE).