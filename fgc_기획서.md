# fgc (Fast Git Cloner) 기획서

**버전**: v0.1 (MVP 중심)  
**작성일**: 2026년 6월 24일  
**목적**: 대규모 Git 저장소 전용 고속 클론 CLI 도구 개발 기획

---

## 1. 프로젝트 개요

### 1.1 기본 정보
- **도구 이름**: `fgc` (Fast Git Cloner)
- **한 줄 소개**: 대규모 Git 저장소의 클론을 **지능적으로 최적화**하여 기존 `git clone` 대비 수 배에서 수십 배 빠르게 수행하는 전용 CLI 도구
- **개발 배경**: 현대 소프트웨어 프로젝트에서 모노레포(Monorepo), 대형 바이너리/에셋, 수십만 커밋 규모 저장소가 증가하면서 표준 `git clone`의 한계가 명확해짐
- **핵심 가치**: 복잡한 Git 고급 기능(`--filter`, `--sparse`, Scalar 등)을 **자동으로 조합**하고, 우수한 사용자 경험(진행률, resume, LFS 최적화)을 제공

### 1.2 타겟 사용자 및 사용 사례
- **주요 타겟**:
  - 대형 오픈소스 프로젝트 기여자 (Linux kernel, Chromium, Android 등)
  - 모노레포를 사용하는 기업/팀 개발자
  - 게임 개발자 (Unity/Unreal 대형 에셋 저장소)
  - ML/AI 연구자 (대형 모델/데이터셋 저장소)
  - CI/CD 파이프라인 구축자
  - 학생/학습자 (큰 저장소를 빠르게 탐색하고 싶을 때)

- **대표 사용 사례**:
  ```bash
  # 일반적인 대형 저장소
  fgc clone https://github.com/torvalds/linux.git

  # 모노레포에서 특정 경로만 빠르게
  fgc clone https://github.com/large-org/monorepo.git --strategy sparse --paths src,docs,tests

  # CI 환경에서 가장 빠른 전략 자동 적용
  fgc clone <url> --strategy auto
  ```

---

## 2. 배경 및 문제 분석

### 2.1 현재 Git clone의 한계
| 문제 | 설명 | 영향 |
|------|------|------|
| 전체 히스토리 다운로드 | 모든 커밋 + 모든 파일 내용(blob) 전송 | 네트워크 대역폭 낭비, 디스크 I/O 증가 |
| 단일 스레드 중심 | Git 내부 다운로드가 충분히 병렬화되지 않음 | 대형 저장소에서 병목 심화 |
| LFS 별도 처리 | `git lfs pull` 이 순차적이고 느림 | LFS 파일 많은 저장소에서 추가 수십 분 소요 |
| Resume 미지원 | 네트워크 끊김 시 대부분 처음부터 재시작 | 불안정한 네트워크 환경에서 큰 고통 |
| 고급 기능의 높은 진입 장벽 | `--filter=blob:none`, `--sparse`, Scalar 등을 조합해야 함 | 일반 개발자가 최적 설정을 모름 |

### 2.2 기존 해결책과 한계
- **Shallow Clone** (`--depth 1`): 빠르지만 히스토리 손실. 나중에 `unshallow` 하면 또 느려짐.
- **Partial Clone** (`--filter=blob:none`): Git 2.23+ 에서 강력 추천. 커밋+트리만 받고, 파일 내용은 필요할 때(on-demand) 다운로드. Linux kernel 기준 기존 15~25분 → 4~6분 수준으로 단축 가능.
- **Sparse Checkout**: 모노레포에서 특정 디렉토리만 체크아웃.
- **Scalar** (Git 내장): Microsoft가 대형 저장소용으로 만든 도구. 부분 클론 + 기타 최적화를 자동 적용.
- **한계**: 위 기능들을 **자동 조합 + 우수한 진행률 + LFS 병렬화 + resume**까지 통합한 도구가 부족함.

**fgc의 차별화 포인트**: "알아서 최적 전략을 골라주고, 진행 상황을 아름답게 보여주며, 중간에 끊겨도 이어서 할 수 있게" 만드는 것.

---

## 3. fgc 핵심 기능 명세

### 3.1 MVP (v0.1) - 반드시 구현할 기능

#### 3.1.1 지능형 전략 자동 선택 엔진
```bash
fgc clone <url>                    # auto 모드 (기본 추천)
fgc clone <url> --strategy blobless
fgc clone <url> --strategy sparse --paths src,app
fgc clone <url> --strategy shallow --depth 1
```

**자동 분석 로직 (MVP)**:
1. 저장소가 public이고 GitHub인 경우 GitHub API로 메타데이터 조회
   - 저장소 전체 크기, LFS 사용 여부, 대략적인 파일 수/커밋 수 추정
2. 휴리스틱 규칙 적용:
   - 저장소 크기 > 2GB 또는 LFS 파일 존재 → `blobless` + `sparse` 추천
   - CI 환경 감지 (`CI=true` 환경변수) → `shallow` 우선
   - `--strategy auto` 명시 시 위 규칙으로 결정

#### 3.1.2 향상된 진행률 표시 (TUI 스타일)
`indicatif` 크레이트를 사용해 다음과 같은 상세 진행률 제공:

```
[1/6] Negotiating refs with remote...          [00:00:03]
[2/6] Receiving objects (42.3 MiB/s)           [██████████░░░░░░░░░░] 52%  1.2GiB / 2.3GiB  ETA 00:01:45
[3/6] Resolving deltas                         [███████████████░░░░░] 78%
[4/6] LFS files (parallel 8)                   [█████░░░░░░░░░░░░░░░] 31%  124MiB / 400MiB
[5/6] Checking out files                       [████████████████████] 100%
[6/6] Post-processing (config, hooks)          done

Total time: 4m 12s  |  Final size: 1.8 GiB (partial clone)
```

#### 3.1.3 LFS 최적화
- `git-lfs` 설치 여부 자동 확인 및 안내
- LFS 파일 목록을 먼저 받아온 후 **병렬 다운로드** (기본 4~8 스레드, `--lfs-jobs` 옵션 지원)
- 가능하면 `aria2c` 연동으로 더 빠른 다중 연결 다운로드 지원 (선택)

#### 3.1.4 기본 Resume 지원
- 클론 진행 중 `.git/fgc-state.json` 또는 별도 상태 파일에 현재 단계와 다운로드된 객체 정보 기록
- 동일한 `fgc clone` 명령 재실행 시 "이어서 진행하시겠습니까?" 확인 후 resume
- 네트워크 불안정 환경에서 실용성 크게 향상

### 3.2 Phase 2+ 확장 기능 (우선순위 순)

| 우선순위 | 기능 | 설명 |
|----------|------|------|
| 높음 | `fgc benchmark <url>` | 여러 전략(full/shallow/blobless/sparse)을 실제로 실행해보고 시간/크기 비교 테이블 출력 |
| 높음 | 로컬 참조 저장소 지원 | `--reference ~/.cache/fgc/main` 로 공통 객체 재사용 (여러 저장소 클론 시 대폭 단축) |
| 중간 | 상세 설정 파일 | `~/.config/fgc/config.toml` 에서 기본 전략, LFS 병렬 수, mirror 우선순위 등 설정 |
| 중간 | TUI 전체 화면 모드 | `ratatui` 사용한 예쁜 대시보드 (속도 그래프 포함) |
| 낮음 | `fgc fetch` / `fgc pull` 최적화 | clone 이후에도 fetch/pull 시 부분 클론 친화적으로 동작하도록 래핑 |
| 낮음 | Mirror 자동 선택 | 지역별 빠른 mirror (한국 사용자 대상) 자동 추천 |

---

## 4. 기술 스택 및 아키텍처

### 4.1 추천 기술 스택

| 영역 | 기술 | 이유 |
|------|------|------|
| 언어 | **Rust** (강력 추천) | 단일 바이너리 배포, 높은 성능, 메모리 안전, CLI 생태계 우수 (clap, indicatif, tokio) |
| 대안 언어 | Go | 동시성 구현이 매우 쉽고, Rust보다 학습 곡선이 완만 |
| CLI 파싱 | `clap` (Rust) | derive 매크로로 타입 안전한 옵션 정의 |
| 진행률 | `indicatif` | 예쁘고 커스터마이징 쉬운 progress bar/spinner |
| 비동기/네트워킹 | `tokio` + `reqwest` | GitHub API 호출, LFS 다운로드 병렬화 |
| Git 제어 (MVP) | `std::process::Command` | 가장 안전하고 빠르게 개발 가능. stdout 파싱으로 진행률 추출 |
| 고도화 시 | `git2` crate 또는 직접 pack protocol | 더 세밀한 제어가 필요할 때 |
| 설정/상태 | `serde` + `toml` / `json` | 설정 파일과 resume 상태 저장 |
| LFS | `git lfs` 명령 호출 + 필요시 직접 HTTP | 기존 LFS 클라이언트 재사용 |

### 4.2 아키텍처 개요 (MVP)

```
┌─────────────────────────────────────────────────────────────┐
│                        fgc CLI (clap)                        │
└────────────────────────────┬────────────────────────────────┘
                             │
         ┌───────────────────┴───────────────────┐
         ▼                                       ▼
┌──────────────────────┐              ┌──────────────────────┐
│  Strategy Analyzer   │              │   Progress Monitor   │
│  (GitHub API 조회 +  │              │   (indicatif +       │
│   휴리스틱 규칙)      │              │    stdout parser)     │
└──────────┬───────────┘              └──────────┬───────────┘
           │                                     │
           ▼                                     ▼
┌─────────────────────────────────────────────────────────────┐
│              Git Command Executor (Command)                  │
│   git clone --filter=blob:none --sparse ...                  │
│   + LFS 병렬 pull + resume state 관리                        │
└─────────────────────────────────────────────────────────────┘
```

**개발 전략**: 
- **MVP 단계**에서는 `git` 바이너리를 최대한 활용하는 **래퍼 + 스마트 오케스트레이터** 형태로 개발 (안정성 최우선)
- 부분 클론의 실제 객체 다운로드(on-demand) 동작은 Git이 알아서 처리하므로, fgc는 "최적의 시작 조건"을 만들어주는 역할에 집중

---

## 5. 개발 로드맵 (개인 프로젝트 기준)

### Phase 1: MVP 핵심 (목표: 2~3주)
- [ ] Rust 프로젝트 초기화 + `clap` 기본 구조
- [ ] `fgc clone <url>` 기본 동작 (git 명령 래핑)
- [ ] `indicatif` 를 이용한 진행률 표시 (객체 수신 단계 파싱)
- [ ] `--strategy` 옵션 구현 (auto 포함)
- [ ] GitHub API 연동 (public repo 메타데이터 조회)
- [ ] LFS 자동 감지 및 `git lfs pull` 호출
- [ ] 기본 resume 로직 (상태 파일 저장/로드)

### Phase 2: 실용성 강화 (목표: 추가 2주)
- [ ] `fgc benchmark` 명령 구현
- [ ] `--reference` 지원 (로컬 객체 캐시)
- [ ] 에러 메시지 및 fallback 로직 강화 (partial clone 미지원 서버 대응)
- [ ] 설정 파일 지원
- [ ] README + 사용 예시 문서화

### Phase 3: 고도화 및 릴리즈 (선택)
- [ ] ratatui 기반 TUI 모드
- [ ] aria2c 연동으로 LFS 초고속 다운로드
- [ ] GitHub Releases를 통한 바이너리 배포 (`cargo install` 지원)
- [ ] 오픈소스화 (MIT/Apache 2.0)

---

## 6. 성능 목표 (측정 가능 지표)

| 저장소 예시              | 표준 `git clone` | fgc 목표 (auto 전략) | 주요 적용 기술          |
|--------------------------|------------------|----------------------|-------------------------|
| Linux Kernel            | 15~25분         | 4~7분               | blobless + LFS 최적화  |
| 대형 모노레포 (8~15GB)  | 30~60분+        | 8~15분              | blobless + sparse      |
| LFS 500개+ 포함 저장소  | LFS pull 별도 소요 | LFS 2~4배 빠름     | 병렬 LFS 다운로드      |
| CI 환경 (shallow 추천)  | 3~8분           | 1~3분               | shallow + single-branch| 

**성공 기준**:
- Linux kernel blobless clone 기준 **5분 이내** 안정 달성
- 네트워크 중단 후 resume 성공률 90% 이상
- "이게 왜 이제야 나왔지?" 수준의 UX 만족도

---

## 7. 위험 요소 및 대응 방안

| 위험 요소                        | 확률 | 영향 | 대응 방안 |
|----------------------------------|------|------|-----------|
| Partial clone 미지원 서버 (구형 GitLab 등) | 중   | 중   | 전략 fallback + "이 서버는 blobless를 지원하지 않습니다. shallow clone으로 전환합니다." 명확한 메시지 |
| `git` stdout 파싱 불안정         | 중   | 중   | 정규식 + 상태 머신으로 robust하게 파싱. 실패 시 일반 git 진행률로 fallback |
| LFS resume 구현 복잡도           | 저   | 저   | git-lfs 자체 resume 기능 최대한 활용 + 파일 존재 여부 체크 |
| Rust 학습 곡선 (Python 배경)     | 중   | 중   | MVP는 Python prototype으로 먼저 검증 후 Rust로 포팅 고려 (또는 Go 선택) |
| Scalar와의 중복성                | -    | -    | fgc는 **자동화 + UX + LFS + resume**에 특화. Scalar는 저수준 엔진으로 보고 상호 보완 가능 |

---

## 8. 기대 효과 및 포트폴리오 가치

### 8.1 실사용 가치
- 개인 개발자: 대형 저장소 참여 장벽 대폭 감소
- 팀/CI: 파이프라인 실행 시간 단축 → 인프라 비용 절감
- 학습자: 큰 오픈소스 코드를 빠르게 로컬에 받아서 공부 가능

### 8.2 포트폴리오 / 커리어 관점 (보안 전공 준비생에게)
- **실제 동작하는 CLI 도구** 개발 경험 (Python → Rust 전환 가능성)
- Git 내부 동작(객체 모델, partial clone, pack protocol)에 대한 깊은 이해
- 네트워킹, 병렬 처리, 상태 관리, 사용자 경험 등 실무적 역량 입증
- "대규모 시스템의 신뢰성/성능 최적화" 관점에서 보안 엔지니어에게도 연결되는 주제 (공격 표면 감소, 안정적 인프라 구축 등)

---

## 9. 결론 및 즉시 시작 가이드

`fgc`는 단순한 `git` 래퍼가 아니라, **대규모 Git 저장소와 개발자가 더 쾌적하게 상호작용할 수 있게 만드는 실용 도구**입니다.

### 지금 당장 시작할 수 있는 단계

1. **Rust 프로젝트 생성**
   ```bash
   cargo new fgc --bin
   cd fgc
   ```

2. **MVP 첫 번째 목표** (오늘/내일 할 수 있는 일)
   - `clap`으로 `fgc clone <url>` 파싱
   - `indicatif` 추가하고 간단한 spinner + progress bar 테스트
   - `std::process::Command` 로 `git clone` 을 호출하면서 stdout을 실시간으로 읽어 progress bar 업데이트하는 POC 만들기

3. **필요하면 추가 지원 요청**
   - Rust 코드 스켈레톤 (main.rs 구조)
   - 특정 기능 상세 설계 (resume 로직, GitHub API 연동 등)
   - Python prototype 버전 먼저 만들기

이 기획서를 바탕으로 개발을 진행하면 체계적이고 방향성 있는 결과물이 나올 거예요.

필요한 부분 더 구체화하거나, 코드 예시가 필요하면 언제든 말해주세요!

---

**부록: 참고 자료**
- Git 공식 Partial Clone 문서: https://github.blog/open-source/git/get-up-to-speed-with-partial-clone-and-shallow-clone/
- Scalar 관련: Git 2.38+ 내장 (`git scalar clone`)
- 대형 저장소 최적화 사례: Linux kernel, Chromium, Windows monorepo 등

이 문서는 fgc 개발의 기준이 될 수 있도록 작성되었습니다.