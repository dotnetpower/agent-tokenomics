# agent-tokenomics

LLM 토큰을 절약하는 실용적인 방법을 두 개의 트랙으로 정리한 문서 저장소입니다.
[microsoft/TokenEconomizer](https://github.com/microsoft/TokenEconomizer)에서 영감을 받았습니다.

- **트랙 A – Copilot 토큰 절약**: VS Code의 GitHub Copilot에서 토큰과 AI 크레딧 사용량을 줄입니다.
- **트랙 B – 에이전트 토큰 절약**: LLM 에이전트를 위한 문제 해결 절차입니다.
  진단 → 격리 → 증명 → 개선 → 결과 순서로 진행합니다.

문서 사이트(Astro Starlight, GitHub Pages): <https://dotnetpower.github.io/agent-tokenomics/ko/>

영문 README는 [README.md](README.md)를 참조하세요.

## 저장소 구조

| 경로 | 내용 |
|------|------|
| `site/` | Astro Starlight 문서 사이트입니다. 영어 문서는 `site/src/content/docs/`, 한국어 문서는 `site/src/content/docs/ko/`에 있습니다. |
| `examples/` | `token_report.py`는 JSONL 요청 로그를 요약합니다(오버헤드, 캐시 적중, 사용되지 않는 도구, 실패 루프). `sample-requests.jsonl`은 샘플 로그입니다. `count_tokens.py`는 한 문장의 언어별 토큰 수를 셉니다(`tiktoken` 필요). `formula_prompt_en.py`(영문 전용), `formula_prompt_ko.py`(한국어 전용), `formula_prompt.py`(통합 실행기)와 `test_formula_prompt.py`는 서술형, 유니코드 수학 기호, BPE 정렬 수식 프롬프트 및 Hugging Face 공개 상담·고객지원 대화 이력 압축의 토큰 수와 슬롯 보존율을 언어별로 분리해 비교·검증합니다(`tiktoken` 필요). `prompt_budget.py`와 `sample-pipeline.json`은 다단계 파이프라인의 단계별 토큰 지도를 정적으로 추정합니다(합성 예시). `papers.json`, `papers-ko.json`, `fetch_paper_meta.py`, `build_papers.py`는 논문 순위 페이지를 생성합니다. |
| `profiler/` | [`tokprof`](https://pypi.org/project/tokprof/): 코드를 수정하지 않고 세션별·턴별·호출별 입력 토큰을 시스템 프롬프트, 대화 이력, 사용자 프롬프트, 도구 관련 항목으로 분해하는 Rust 토큰 프로파일러입니다(`pip install tokprof` / `uvx tokprof`). |
| `.github/copilot-instructions.md` | 이 저장소의 문서 작성 규칙입니다. |

## 개발

```bash
npm --prefix site ci
npm --prefix site run dev      # http://localhost:4321
npm --prefix site run build
python3 examples/token_report.py examples/sample-requests.jsonl
```

사이트 로고는 **T**에 화폐 기호를 연상시키는 두 개의 사선을 더해 토큰과 비용을 표현합니다. 헤더 로고와 favicon은 [`site/public/favicon.svg`](site/public/favicon.svg)를 함께 사용하므로, 이 SVG를 수정하면 두 곳에 모두 반영됩니다.

## 라이선스

[LICENSE](LICENSE)를 참조하세요.
