# Darkstar: Professional OWL 2 RL Ontology Editor

[English](#english) | [한국어](#한국어)

<a name="english"></a>
## English

Darkstar is a high-performance, professional-grade ontology editor and inference engine built with Rust. It provides a visual and intuitive way to manage complex OWL 2 RL ontologies with real-time reasoning and dynamic graph visualization.

![Darkstar Icon](assets/icon.png)

### Key Features

- **OWL 2 RL Compliance**: Full support for OWL 2 RL inference rules, including property chains, class expressions, and consistency checking.
- **Real-time Reasoning**: Automatic incremental reasoning as you edit your ontology.
- **Dynamic Graph Visualization**: Interactive, force-directed graph view with multiple layout modes (Hierarchical, Radial).
- **Bilingual Support**: Full dynamic support for English and Korean (한국어) languages.
- **Cross-Platform**: Built with Rust and `egui` for a native experience on macOS and Windows.
- **Visual Editing**: Manage classes, properties, and individuals through a modern, docked UI.

### Tech Stack

- **Core**: [Rust](https://www.rust-lang.org/)
- **UI Framework**: [egui](https://github.com/emilk/egui) / [eframe](https://github.com/emilk/egui/tree/master/crates/eframe)
- **RDF/Ontology**: [Sophia](https://github.com/pchampin/sophia_rs)
- **Parallelism**: [Rayon](https://github.com/rayon-rs/rayon)

---

<a name="한국어"></a>
## 한국어

Darkstar는 Rust로 제작된 고성능 전문가용 온톨로지 에디터 및 추론 엔진입니다. 실시간 추론과 역동적인 그래프 시각화를 통해 복잡한 OWL 2 RL 온톨로지를 직관적으로 관리할 수 있습니다.

### 주요 기능

- **OWL 2 RL 준수**: 속성 체인(Property chains), 클래스 표현식, 일관성 검사를 포함한 OWL 2 RL 추론 규칙을 완벽하게 지원합니다.
- **실시간 추론**: 온톨로지를 편집함에 따라 자동으로 증분 추론(Incremental reasoning)이 수행됩니다.
- **역동적인 그래프 시각화**: 힘 지향(Force-directed) 그래프와 다양한 레이아웃 모드(계층형, 방사형)를 지원하는 대화형 뷰를 제공합니다.
- **이국어 지원**: 영어와 한국어를 동적으로 전환하며 사용할 수 있습니다.
- **크로스 플랫폼**: Rust와 `egui`를 사용하여 macOS와 Windows에서 네이티브 경험을 제공합니다.
- **시각적 편집**: 현대적인 도킹 UI를 통해 클래스, 속성, 개체를 효율적으로 관리할 수 있습니다.

### 기술 스택

- **핵심 언어**: [Rust](https://www.rust-lang.org/)
- **UI 프레임워크**: [egui](https://github.com/emilk/egui) / [eframe](https://github.com/emilk/egui/tree/master/crates/eframe)
- **RDF/온톨로지**: [Sophia](https://github.com/pchampin/sophia_rs)
- **병렬 처리**: [Rayon](https://github.com/rayon-rs/rayon)

---

## Getting Started / 시작하기

### Prerequisites / 사전 요구사항

- [Rust](https://www.rust-lang.org/tools/install) (latest stable version)

### Build and Run / 빌드 및 실행

```bash
# 저장소 클론
git clone https://github.com/tsarkr/darkstar.git
cd darkstar

# 실행 (Release 모드 권장)
cargo run --release
```

### Packaging / 패키징

운영체제(macOS/Windows)에 맞는 설치 파일을 만들려면 아래 스크립트를 실행하세요:

```bash
./package.sh
```

## License / 라이선스

This project is licensed under the MIT License - see the LICENSE file for details.

## Authors / 작성자

- **Darkstar Team** - *Professional OWL 2 RL Ontology Editor*
