# Darkstar: Professional OWL 2 RL Ontology Editor

<p align="center">
  <img src="assets/logo.png" width="300" alt="Darkstar Logo">
</p>

Darkstar is a high-performance, professional-grade ontology editor and inference engine built with Rust. It provides a visual and intuitive way to manage complex OWL 2 RL ontologies with real-time reasoning and dynamic graph visualization.

[English](#english) | [한국어](#한국어)

---

<a name="english"></a>
## English

### Key Features

- **Professional Reasoning Engine**: OWL 2 RL compliance with high-performance parallel forward-chaining using [Rayon](https://github.com/rayon-rs/rayon).
- **Advanced Graph Visualization**: Interactive graph view with multiple layouts (Force-Directed, Hierarchical, Radial, Circular, Concentric, and Grid).
- **Protégé-Inspired Workflow**: Familiar refactoring tools including **Rename Entity** and **Merge Entities** with full safety validation.
- **Modern User Experience**: 
    - Stunning **Splash Screen** and card-style **Onboarding Wizard**.
    - Fully localized in **English and Korean**.
    - **System Clipboard Integration** for seamless URI copying across applications.
    - Responsive UI with **Dark/Light Mode** support and UI scaling.
- **Data Integrity**: Automated ontology metrics, consistency checking, and safety confirmation modals to prevent data loss.

### Tech Stack

- **Core**: [Rust](https://www.rust-lang.org/)
- **UI Framework**: [egui](https://github.com/emilk/egui) / [eframe](https://github.com/emilk/egui/tree/master/crates/eframe)
- **RDF/Ontology**: [Sophia](https://github.com/pchampin/sophia_rs)
- **Image Processing**: [egui_extras](https://github.com/emilk/egui/tree/master/crates/egui_extras) with image loaders.

---

<a name="한국어"></a>
## 한국어

### 주요 기능

- **전문가용 추론 엔진**: [Rayon](https://github.com/rayon-rs/rayon)을 이용한 고성능 병렬 전방 추론(Forward-Chaining)으로 OWL 2 RL 규칙을 완벽하게 지원합니다.
- **고급 그래프 시각화**: 힘 지향(Force-Directed), 계층형, 방사형, 원형, 동심원 및 그리드 등 다양한 레이아웃을 지원하는 대화형 뷰를 제공합니다.
- **Protégé 스타일 워크플로우**: 엔티티 이름 변경(Rename) 및 병합(Merge) 기능을 포함한 강력하고 안전한 리팩토링 도구를 제공합니다.
- **현대적인 사용자 경험**:
    - 세련된 **스플래시 화면**과 카드 스타일의 **초기 설정 위저드**.
    - **영어 및 한국어** 완전 지원 (동적 언어 전환).
    - 앱 간 원활한 URI 복사를 위한 **시스템 클립보드 연동**.
    - **다크/라이트 모드** 및 UI 배율 조절을 지원하는 반응형 UI.
- **데이터 무결성**: 자동 온톨로지 통계, 일관성 검사 및 안전 확인 모달을 통한 데이터 보호.

### 기술 스택

- **핵심 언어**: [Rust](https://www.rust-lang.org/)
- **UI 프레임워크**: [egui](https://github.com/emilk/egui)
- **RDF/온톨로지**: [Sophia](https://github.com/pchampin/sophia_rs)
- **이미지 처리**: `egui_extras` (image feature)

---

## Getting Started / 시작하기

### Prerequisites / 사전 요구사항

- [Rust](https://www.rust-lang.org/tools/install) (latest stable version)

### Build and Run / 빌드 및 실행

```bash
# 저장소 클론
git clone https://github.com/tsarkr/darkstar.git
cd darkstar

# 실행 (최초 실행 시 온보딩 위저드가 나타납니다)
cargo run --release
```

### Packaging / 패키징

운영체제(macOS/Windows)에 맞는 설치 파일을 만들려면 아래 스크립트를 실행하세요:

```bash
./package.sh
```

## License / 라이선스

Apache License 2.0 or MIT (Dual Licensed)

## Authors / 작성자

- **Gyungmin** (@tsarkr)
