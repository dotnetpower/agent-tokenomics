---
created: 2026-11-19
updated: 2026-11-19
dateSource: recorded
title: "코딩 에이전트 토큰·캐시·구독 한도 해부 (Copilot · Codex · Claude)"
description: "GitHub Copilot, OpenAI Codex, Claude Code의 과금·캐시 동작, 사용량 진단, 세션 관리, 구독 한도를 비교합니다."
track: agent
verified: 2026-11-19
sidebar:
  order: 7
---

코딩 에이전트의 요금제 한도가 예상보다 빨리 소진되는 이유를 파악하려면 **총토큰 수**, **API 청구 비용**, **구독 한도 차감**을 구분해야 합니다. 이 문서는 **GitHub Copilot**(VS Code · CLI · SDK), **OpenAI Codex**(CLI · IDE · Cloud), **Claude Code**의 공식 문서를 바탕으로 토큰과 한도의 관계를 비교합니다.

> **작성·검증 기준일: 2026-11-19.** 가격, 구독 한도, 지원 모델, 캐시 동작은 이후 변경될 수 있습니다. 이 문서는 공개 문서 검토 결과이며 실제 청구를 독립적으로 재현한 실험이 아닙니다. 구매·운영 결정 전에는 본문의 원문 링크와 본인 계정의 사용량 화면을 다시 확인해야 합니다. API 단가가 구독 한도 차감 방식에 그대로 적용되는 것은 아닙니다.

<a id="total-tokens-vs-cost-vs-quota"></a>
## 1. 핵심 명제: 총토큰 수 ≠ API 비용 ≠ 구독 한도 차감

단일 작업의 총토큰 비율을 비용이나 효율의 비율로 해석할 수는 없습니다. 작업, 모델, 추론 설정, 캐시 집계 방식, 성공 기준을 맞춘 뒤 비교해야 합니다.

코딩 에이전트가 **탐색 → 파일 읽기 → 수정 → 테스트 실행 → 실패 시 재읽기**를 반복하면 이전 대화와 도구 결과가 후속 요청의 논리적 입력에 포함됩니다. 이때 같은 접두부는 새로 연산하는 대신 프롬프트 캐시에서 읽을 수 있습니다. 다만 모든 클라이언트가 매번 전체 바이트를 네트워크로 재전송한다는 뜻은 아닙니다. 연속 요청 API와 지속 연결은 전송 오버헤드를 줄일 수 있습니다([Claude Code 요청 구조](https://code.claude.com/docs/en/prompt-caching#how-the-cache-is-organized), [VS Code 엔지니어링 설명](https://code.visualstudio.com/blogs/2026/06/17/improving-token-efficiency-in-github-copilot)).

<figure class="at-doc-figure">
  <div class="at-diag-stack">
    <div class="at-diag-row">
      <span>1. 신규 입력 + 캐시 쓰기</span>
      <div class="at-diag-bar">
        <div class="at-diag-seg seg-miss" style="width:100%">일반 입력과 캐시 쓰기를 구분합니다</div>
      </div>
      <span>유형별 입력 단가 적용</span>
    </div>
    <div class="at-diag-row">
      <span>2. 프롬프트 캐시 읽기</span>
      <div class="at-diag-bar">
        <div class="at-diag-seg seg-hit" style="width:100%">재사용 접두부에 모델별 읽기 단가 적용</div>
      </div>
      <span>토큰 수와 단가 분리</span>
    </div>
    <div class="at-diag-row">
      <span>3. 청구 출력(추론 포함)</span>
      <div class="at-diag-bar">
        <div class="at-diag-seg seg-tool" style="width:100%">추론을 포함한 공식 출력 집계 사용</div>
      </div>
      <span>추론 중복 합산 금지</span>
    </div>
  </div>
  <figcaption>
    그림 1. 실측 비율이 아닌 개념적 토큰 분류입니다. 막대 너비가 같다고 토큰 수, 비용, 구독 차감 비중이 같다는 뜻은 아닙니다.
  </figcaption>
</figure>

총 $T$번의 모델 호출에 대한 텍스트 토큰 비용은 중복되지 않는 항목으로 나누고, 모델·컨텍스트 구간·캐시 수명에 맞는 단가를 적용해 계산합니다.

```text
TextTokenCost = Σ_t ( C_in · UncachedIn_t + C_write · CacheWrite_t + C_read · CacheRead_t + C_out · BilledOut_t )
```

- **입력을 중복 합산하지 않습니다:** 제공자별 사용량 필드를 먼저 위 항목으로 정규화해야 합니다. 캐시 쓰기 단가는 해당 토큰에 적용하는 전체 단가이지, 일반 입력 단가에 쓰기 단가 전체를 추가하는 요금이 아닙니다. 보존 시간별 단가가 다르면 쓰기 항목도 나눕니다. 도구 호출료 등 별도 서비스 요금은 이 텍스트 토큰 산식 밖에서 계산합니다([Anthropic 캐시 요금](https://platform.claude.com/docs/en/build-with-claude/prompt-caching#pricing), [OpenAI 캐시 과금 규칙](https://developers.openai.com/api/docs/guides/prompt-caching)).
- **추론을 중복 합산하지 않습니다(`BilledOut_t`):** 제공자가 보고한 `output_tokens`를 사용합니다. OpenAI의 추론 하위 필드는 `output_tokens_details.reasoning_tokens`이며, Anthropic도 사용자에게 보이지 않는 사고를 포함해 출력으로 과금합니다. 이미 추론을 포함한 출력에 추론을 더하거나, 공식 필드 정의 대신 `max(output_tokens, reasoning_tokens)`로 계산하지 않습니다([OpenAI 추론 사용량](https://developers.openai.com/api/docs/guides/reasoning), [Anthropic 사고 토큰 과금](https://platform.claude.com/docs/en/build-with-claude/thinking)).

---

<a id="three-agent-comparison"></a>
## 2. 3대 코딩 에이전트 과금·캐시·한도 구조 비교 (2026-11-19 기준)

아래 표는 **모델 제공자의 API 단가**와 **제품의 구독·크레딧 규칙**을 구분합니다. API 동작을 모든 클라이언트·게이트웨이·구독에 일반화하면 안 됩니다. 달러로 표시한 요금제 가격은 해당 세금을 제외한 월간 USD 정가입니다.

| 비교 항목 | GitHub Copilot (VS Code · CLI · SDK) | OpenAI Codex (CLI · IDE · Cloud) | Claude Code (Pro / Max / Ultra) |
|---|---|---|---|
| **요금제·한도 방식** | 사용량 기반 요금제는 모델별 토큰 요금을 **GitHub AI Credits(`1 크레딧 = $0.01 USD`)**로 환산합니다. 개인별 제공량과 조직 공유 풀은 다릅니다([개인 과금](https://docs.github.com/en/copilot/concepts/billing-and-usage/individuals/billing), [조직 과금](https://docs.github.com/en/copilot/concepts/billing-and-usage/organizations-and-enterprises/billing)). | Plus·Standard Business는 5시간 구간별 사용량 예시가 있지만, **Pro는 현재 5시간 제한이 없습니다.** 로컬 메시지와 클라우드 대화는 한도를 공유하며 주간 한도가 적용될 수 있습니다. API 키 인증은 별도 API 과금입니다([가격과 한도](https://learn.chatgpt.com/docs/pricing)). | **Max(`$100`)**: 5시간·주간 모두 Pro의 `5배`입니다. **Ultra(`$200`)**: 주간 Pro의 `10배`이며, 인증 계정은 5시간 제한이 없습니다([요금제와 조건](https://claude.com/pricing)). |
| **프롬프트 캐시 읽기 단가** | 제공자의 할인율을 일괄 가정하지 않고 **GitHub가 공개한 모델·컨텍스트 구간별 단가**를 사용합니다([모델별 가격](https://docs.github.com/en/copilot/reference/copilot-billing/models-and-pricing)). | **API:** GPT-5.6 이상 대부분은 `0.10×`, GPT-6.1 Sol/Terra·GPT-6.2 Astra/Sol은 `0.05×`입니다. **Codex 크레딧**은 별도 토큰 요금표를 확인합니다([API 캐싱](https://developers.openai.com/api/docs/guides/prompt-caching), [Codex 요금](https://learn.chatgpt.com/docs/pricing#token-rates)). | **API:** 기본 입력 대비 표준 `0.10×`, Opus 5.5·Sonnet 5.5는 `0.05×`, Fable 5.1/5.2·Mythos 5.1/5.2는 `0.025×`입니다([모델별 요금표](https://platform.claude.com/docs/en/build-with-claude/prompt-caching#pricing)). |
| **프롬프트 캐시 쓰기 단가** | Anthropic과 지원되는 최신 OpenAI 모델의 쓰기 요금이 공개되어 있습니다. 문서화되지 않은 클라이언트 필드로 요금을 추정하지 않습니다([GitHub 요금표](https://docs.github.com/en/copilot/reference/copilot-billing/models-and-pricing)). | **API GPT-5.6 이상:** 기본 `30분` 쓰기 `1.25×`, 요청한 `12시간` 쓰기 `2.5×`입니다. **Codex 크레딧 과금에는 별도 캐시 쓰기 요금이 없습니다**([API 규칙](https://developers.openai.com/api/docs/guides/prompt-caching#cache-lifetime), [Codex 규칙](https://learn.chatgpt.com/docs/pricing#token-rates)). | **API:** `5분` 쓰기는 일반 입력의 `1.25×`, `1시간` 쓰기는 `2.0×`입니다. 일반 입력 요금에 추가로 더하는 금액이 아니라 전체 쓰기 단가입니다([캐시 요금](https://platform.claude.com/docs/en/build-with-claude/prompt-caching#pricing)). |
| **캐시 보존 시간(TTL)** | 클라이언트·모델별로 다릅니다. **2026-06-17 VS Code 보고서**의 OpenAI 확장 보존과 Anthropic 중단점 설명은 모든 Copilot 제품의 영구적인 기본값을 보장하지 않습니다([엔지니어링 보고서](https://code.visualstudio.com/blogs/2026/06/17/improving-token-efficiency-in-github-copilot)). | **API GPT-5.6 이상:** `prompt_cache_options.ttl`로 최소 `30분` 또는 `12시간`을 선택합니다. 이전 모델은 다른 보존 설정을 사용합니다. 이를 모든 Codex 클라이언트의 기본값으로 해석하면 안 됩니다([보존 규칙](https://developers.openai.com/api/docs/guides/prompt-caching#cache-lifetime)). | 주 대화의 기본값은 **Claude 구독의 포함 사용량 안에서는 `1시간`**, 사용량 크레딧·API 키·클라우드 제공자 이용 시에는 `5분`입니다. 서브에이전트는 보통 `5분`이며 재설정할 수 있습니다([TTL 적용 조건](https://code.claude.com/docs/en/prompt-caching#which-ttl-each-request-gets)). |
| **실제 사용량 확인** | 사용량 화면을 본인 요금제·예산과 대조합니다. 조직의 사용자별 예산은 항상 사용을 중단하지만, 지출 한도는 사용 중단 옵션을 켜야 강제됩니다([예산 적용 방식](https://docs.github.com/en/copilot/concepts/billing-and-usage/organizations-and-enterprises/billing#how-can-i-control-costs-with-budgets)). | 계정의 사용량 대시보드에서 현재 한도와 초기화 시점을 확인합니다. 내부 응답 헤더 이름을 영구적인 요금제 계약으로 간주하지 않습니다([사용량 안내](https://learn.chatgpt.com/docs/pricing#included-usage)). | `/usage`의 세션 API 비용 추정치와 구독 사용량 막대를 구분합니다([사용량 화면](https://code.claude.com/docs/en/costs#track-your-costs)). |

**기존 Copilot 연간 구독 예외:** 요청 기반 과금에 남아 있는 Pro/Pro+는 월 `300`/`1,500` 프리미엄 요청을 제공합니다. 사용자 프롬프트에는 모델 배수가 적용되고 자율 도구 호출은 요청 수에 포함되지 않으며, 코드 리뷰는 회당 `13`개 요청을 사용합니다. 이 규칙을 **사용량 기반 토큰 과금에 적용하면 안 됩니다**([기존 요청 과금 규칙](https://docs.github.com/en/copilot/reference/copilot-billing/request-based-billing-legacy/copilot-requests)).

<a id="product-surface-and-authentication"></a>
### 제품·클라이언트·인증 방식의 적용 범위

요금·캐시 규칙을 적용하기 전에 클라이언트, 버전, 하네스, 인증 방식을 기록합니다. 제공자 API의 옵션이 에이전트의 화면이나 설정에 그대로 노출되는 것은 아닙니다.

| 제품 | 기록할 적용 범위 | 피해야 할 가정 |
|---|---|---|
| **GitHub Copilot** | VS Code·CLI·SDK 중 무엇인지, GitHub 과금 모델인지 별도 제공자인지 구분합니다. VS Code는 하네스도 기록합니다. SDK의 **BYOK**는 본인의 제공자 자격 증명으로 요청하고 제공자에게 직접 과금됩니다([SDK BYOK](https://docs.github.com/en/copilot/how-tos/copilot-sdk/auth/byok), [VS Code 하네스 차이](https://code.visualstudio.com/docs/agents/run/subagents)). | SDK BYOK 요금은 GitHub AI Credits와 같은 계기판이 아닙니다. CLI 명령이나 Local 하네스 도구를 모든 VS Code·SDK 세션에 그대로 적용하지 않습니다. |
| **OpenAI Codex** | 로컬 CLI·IDE 작업은 ChatGPT 로그인 또는 API 키를 사용할 수 있지만, **Codex cloud는 ChatGPT 로그인이 필요합니다.** API 키 사용량은 ChatGPT 포함 사용량 대신 Platform 계정으로 과금됩니다([인증 안내](https://learn.chatgpt.com/docs/auth)). | API 캐시 매개변수나 설정 업데이트를 Codex 클라이언트 옵션으로 단정하지 않습니다. 로컬 API 키 접근이 클라우드 기능 접근까지 보장하지는 않습니다. |
| **Claude Code** | Claude 구독·API 키·클라우드 제공자와 구독 포함 사용량·유료 사용량 크레딧을 구분합니다. 이에 따라 과금과 기본 캐시 TTL이 달라집니다([비용 추적](https://code.claude.com/docs/en/costs), [TTL 규칙](https://code.claude.com/docs/en/prompt-caching#which-ttl-each-request-gets)). | 세션 비용 추정치를 구독 한도 차감량으로 해석하거나 구독 주 대화의 TTL을 모든 인증 경로에 적용하지 않습니다. |

<a id="usage-observability"></a>
### 사용량 진단: 어디서 무엇을 확인하는가

| 제품 | 구체적인 확인 경로 | 해석과 한계 |
|---|---|---|
| **GitHub Copilot** | **CLI:** `/context`는 문맥 구성, `/usage`는 세션·모델별 사용량과 토큰 과금 계정의 AI Credits를 보여 줍니다. **VS Code:** **Developer: Open Agent Debug Logs**에서 세션 **Summary**를 열고 **Cache Explorer**로 보고된 재사용과 요청 차이를 확인합니다. **SDK:** `assistant.usage`를 구독하고 문맥 점유율은 `session.usage_info`로 봅니다([CLI 참조](https://docs.github.com/en/copilot/reference/copilot-cli-reference/cli-command-reference), [디버그 화면](https://code.visualstudio.com/docs/agents/agent-troubleshooting/chat-debug-view), [SDK 지표](https://docs.github.com/en/copilot/how-tos/copilot-sdk/features/usage-and-billing)). | 문맥 점유율은 누적 청구 사용량이 아닙니다. 디버그 캡처는 재현 전에 켜야 하며 Preview입니다. Cache Explorer가 제공자의 과금을 증명하지는 않습니다. SDK 사용량 이벤트는 서브에이전트 호출도 포함하고 재개 시 재생되지 않습니다. 누적 지표 RPC는 실험적이므로 SDK·런타임 버전을 고정합니다. |
| **OpenAI Codex** | **CLI:** `/status`로 남은 요금제 한도를 확인하고, 계정 수치는 [공식 안내의 사용량 대시보드](https://learn.chatgpt.com/docs/pricing)에서 봅니다. **자동화:** `codex exec --json`의 `turn.completed.usage`에는 `input_tokens`, `cached_input_tokens`, `output_tokens` 등이 들어 있습니다([JSONL 이벤트](https://learn.chatgpt.com/docs/non-interactive-mode)). | 턴별 토큰 합계는 구독 차감량이나 개별 모델 호출 추적과 다릅니다. 캐시 읽기를 이미 포함한 입력 합계에 다시 더하지 않습니다. 이벤트와 함께 인증 방식도 기록합니다. |
| **Claude Code** | `/usage`로 요금제 막대와 세션 추정치를 봅니다. 구조화된 사용량에서는 `cache_creation_input_tokens`, `cache_read_input_tokens`를 확인합니다([사용량 화면](https://code.claude.com/docs/en/costs#track-your-costs), [캐시 필드](https://code.claude.com/docs/en/prompt-caching#check-cache-performance)). | 구독 막대와 API 달러 추정치를 구분합니다. 캐시 생성·읽기 필드는 토큰 분류이지, 구독 비율로 환산하는 공개 산식이 아닙니다. |

클라이언트·제공자가 공개하지 않는 필드는 0이 아니라 **확인 불가**로 기록합니다. 호출·턴·세션·계정 중 어느 범위의 수치인지 보존하고, 이미 포함된 누적 합계에 이벤트 합계를 다시 더하지 않습니다. 로그를 공유하기 전에는 프롬프트·소스 코드·자격 증명을 검토하고 민감한 내용을 제거합니다([SDK 필드 정의](https://docs.github.com/en/copilot/how-tos/copilot-sdk/features/usage-and-billing), [Cache Explorer의 한계](https://code.visualstudio.com/docs/agents/agent-troubleshooting/cache-explorer)).

---

<a id="cache-illusion-and-subscription-quotas"></a>
## 3. '캐시 착시(Cache Illusion)'와 구독 한도의 불투명성

API 종량제 청구서와 정액 구독 요금제의 사용량 계기판은 같은 방식으로 움직이지 않습니다.

1. **정가 환산이 만드는 캐시 착시:** 캐시 읽기를 모두 새 입력 정가로 환산하면 같은 입력의 API 비용을 부풀리게 됩니다. 차이는 실제 토큰 구성과 모델별 단가에 따라 달라지며, 모든 세션에 적용되는 고정 배수는 없습니다([Anthropic 요금](https://platform.claude.com/docs/en/build-with-claude/prompt-caching#pricing), [OpenAI 규칙](https://developers.openai.com/api/docs/guides/prompt-caching)).
2. **API 추정치는 구독 청구서가 아닙니다:** Claude Code는 Pro/Max/Ultra 구독자에게 세션 비용이 실제 청구 금액이 아니라고 명시합니다. 인용한 요금제 문서에는 캐시 토큰을 한도 차감량으로 바꾸는 고정 산식이 없으므로 이 문서에서도 임의 환산하지 않습니다. Codex도 크레딧 가격만으로 구독 포함 사용량이 결정되지 않는다고 설명합니다([Claude 사용량 안내](https://code.claude.com/docs/en/costs#track-your-costs), [Codex 토큰 요금](https://learn.chatgpt.com/docs/pricing#token-rates)).
3. **단기 제한 폐지는 무제한을 뜻하지 않습니다:** 인증된 Claude Ultra 계정에도 주간 한도가 남습니다. Codex는 Pro의 5시간 제한이 없지만 주간 한도가 적용될 수 있다고 안내합니다. Copilot은 월간 크레딧 제공량과 예산 제어를 사용합니다. 본인 계정의 실제 계기판을 확인해야 합니다([Claude 요금제](https://claude.com/pricing), [Codex 한도](https://learn.chatgpt.com/docs/pricing#included-usage), [Copilot 제공량](https://docs.github.com/en/copilot/concepts/billing-and-usage/individuals/billing)).

<a id="when-allowance-runs-out"></a>
### 포함 사용량을 모두 쓴 뒤의 동작

| 제품 | 한도 도달 → 계속하는 경로 | 주의할 점 |
|---|---|---|
| **GitHub Copilot** | 개인은 추가 유료 사용을 설정하거나 다음 제공량을 기다릴 수 있습니다. 조직 사용자는 관리자의 예산 추가가 필요할 수 있으며, 정책과 지출 한도도 추가 사용을 제어합니다([개인 규칙](https://docs.github.com/en/copilot/concepts/billing-and-usage/individuals/billing), [조직 규칙](https://docs.github.com/en/copilot/concepts/billing-and-usage/organizations-and-enterprises/billing)). | 계속하기 전에 사용 중단 설정을 확인합니다. 인라인 완성과 다음 편집 제안은 AI Credits 과금 대상이 아니며 공식 유료 요금제 규칙에 따라 계속 사용할 수 있습니다. 채팅 예산 중단이 모든 기능 중단을 뜻하지는 않습니다. |
| **OpenAI Codex** | 해당 Plus/Pro 사용자는 크레딧을 구매할 수 있고, 유연한 과금의 워크스페이스는 공유 크레딧을 추가할 수 있습니다. 진행 중인 턴은 공정 사용 한도 안에서 계속될 수 있습니다. 추가 로컬 API 키 작업은 별도 API 과금입니다([한도 도달 처리](https://learn.chatgpt.com/docs/pricing), [인증 적용 범위](https://learn.chatgpt.com/docs/auth)). | API 키 과금은 ChatGPT 제공량을 충전하거나 Codex cloud를 열어 주지 않습니다. 허용된 계속 사용 경로가 없다면 계정의 초기화 시점을 확인합니다. |
| **Claude Code** | 설정된 사용량 크레딧으로 구독 포함량을 넘는 유료 작업을 이어갈 수 있으며, 그 외에는 요금제의 초기화·업그레이드 경로를 따릅니다. `/usage`에서 요금제 사용량과 사용량 크레딧 지출을 구분합니다([비용·사용량 제어](https://code.claude.com/docs/en/costs)). | 세션의 API 달러 추정치를 구독 잔액으로 해석하지 않습니다. 사용량 크레딧으로 전환하면 별도 설정이 없는 한 주 대화의 기본 TTL도 달라집니다. |

---

<a id="six-token-drain-mechanisms"></a>
## 4. 실제 토큰과 한도를 빠르게 태우는 6가지 메커니즘

<a id="1-search-scope-shell-dumps-and-retry-loops"></a>
### 4.1 도구 이름이 아니라 '탐색 범위·셸 출력·재시도 횟수'가 결정한다

도구 이름만으로 효율을 판단할 수는 없습니다. 범위를 제한하지 않은 파일·검색 출력은 후속 요청을 키울 수 있고, 지나치게 짧은 조회는 버그 수정에 필요한 정의나 호출부를 빠뜨릴 수 있습니다. 결과가 모델의 문맥에 처음 들어가기 전에 출력 범위를 제한하되, 불필요한 재시도를 피할 만큼은 읽어야 합니다([Claude 비용 절감 안내](https://code.claude.com/docs/en/costs#reduce-token-usage)).

[`cctx`](../../copilot-tokenomics/cctx/)와 같이 필요한 정의·호출부를 결과 범위와 함께 조회하는 방식을 활용할 수 있습니다. 단, **과거 도구 결과를 임의로 줄여 기존 대화에 덮어쓰면 안 됩니다.** 접두부 변경은 캐시 재사용을 줄이고 최신 OpenAI 모델에서는 추론 연속성에도 영향을 줄 수 있습니다. 새 결과를 미리 필터링하거나 제공자가 지원하는 압축 기능을 사용합니다([OpenAI 접두부 유지 안내](https://developers.openai.com/api/docs/guides/keep-prefix-unchanged)).

제품별 출력 제어도 다릅니다. Copilot CLI는 큰 도구 결과를 임시 파일로 분리할 수 있고, Codex는 MCP 도구별 `output_token_limit`을 지원합니다. 어느 경우에도 실패 상태·경고와 진단에 필요한 문맥은 보존해야 합니다([Copilot 출력 처리](https://docs.github.com/en/copilot/concepts/prompting-and-context/context-management), [Codex MCP 제어](https://learn.chatgpt.com/docs/extend/mcp?surface=cli)).

<a id="2-standing-prompt-and-schema-tax"></a>
### 4.2 상주 프롬프트와 MCP 도구 스키마 세금 (`고정 토큰 × 호출 수`)

시스템 프롬프트, 이미 로드한 프로젝트 지침, 즉시 로드하는 도구 스키마는 여러 호출에 반복 포함될 수 있습니다. 하지만 **활성화된 도구가 모두 첫 요청에 전체 스키마로 들어간다는 뜻은 아닙니다.** 지연 로딩은 쓰지 않는 스키마를 문맥 밖에 둘 수 있습니다. VS Code 보고서는 캐시 접두부 뒤에서 도구를 검색하는 방식을 설명하며, Claude Code 문서는 지연 로딩된 MCP 도구 변경이 캐시를 보존하는 조건을 명시합니다([VS Code 도구 검색](https://code.visualstudio.com/blogs/2026/06/17/improving-token-efficiency-in-github-copilot), [Claude MCP 캐시 규칙](https://code.claude.com/docs/en/prompt-caching#connecting-or-removing-an-mcp-server)).

상주 지침은 짧게 유지하고, 지원되는 환경에서는 선택적 도구를 필요할 때 로드합니다. 특정 클라이언트의 스키마 배치나 절감률을 모든 제품에 일반화하지 않습니다.

| 제품 | 지침·도구 제어 | 확인 방법 |
|---|---|---|
| **GitHub Copilot** | CLI의 `copilot instruction list`로 발견한 지침을 나열하고, `/mcp`에서 무제한 `*` 대신 사용할 도구 이름과 서버를 설정합니다([지침 참조](https://docs.github.com/en/copilot/reference/copilot-cli-reference/cli-command-reference), [MCP 설정](https://docs.github.com/en/copilot/how-tos/copilot-cli/customize-copilot/add-mcp-servers)). | `/context`에서 지침·MCP 비중을 봅니다. VS Code는 Chat Debug의 실제 요청을 확인합니다. 지침 발견만으로 요청 포함이 증명되지는 않습니다. |
| **OpenAI Codex** | `AGENTS.md`·`AGENTS.override.md`가 시작 시 지침 체인을 구성합니다. MCP의 `enabled_tools`·`disabled_tools`로 목록을 제한하고 `enabled = false`로 서버를 끕니다([지침 발견 규칙](https://learn.chatgpt.com/docs/agent-configuration/agents-md), [MCP 옵션](https://learn.chatgpt.com/docs/extend/mcp?surface=cli)). | 시작 시 지침을 바꿨다면 대상 디렉터리에서 재시작하고 `/mcp`와 로드한 지침 출처를 확인합니다. 클라이언트 도구 목록만으로 API의 `defer_loading` 동작을 추정하지 않습니다. |
| **Claude Code** | `CLAUDE.md`는 프로젝트 문맥에 로드되며 MCP 도구 검색은 정의를 지연 로딩할 수 있습니다. 즉시 로드한 도구 목록 변경과 지연 로딩된 변경은 캐시 영향이 다릅니다([프롬프트 계층과 MCP 규칙](https://code.claude.com/docs/en/prompt-caching)). | 보고된 캐시 생성·읽기 사용량과 로드한 도구를 대조합니다. 권한 규칙이 곧 도구 스키마 제거를 뜻하지는 않습니다. |

<a id="3-silent-cache-invalidators"></a>
### 4.3 캐시 재사용을 줄일 수 있는 3가지 요인과 예외

1. **모델·추론 설정 변경:** 캐시는 모델별로 구분됩니다. 추론 강도 변경도 접두부를 바꿀 수 있지만 **항상 그런 것은 아닙니다.** Claude Code는 지원되는 최신 모델과 인증 경로에서 캐시를 유지하는 예외를 명시합니다. 지원되는 OpenAI 모델은 요청 수준의 기존 `reasoning.effort`를 유지한 채 `configuration_update`를 뒤에 추가할 수 있습니다([Claude 예외 조건](https://code.claude.com/docs/en/prompt-caching#changing-effort-level), [OpenAI 추가 방식 업데이트](https://developers.openai.com/api/docs/guides/keep-prefix-unchanged)). 모든 변경이 전체 미적중을 일으킨다고 가정하지 말고 클라이언트 구현을 확인합니다.
2. **유휴 간격·보존 설정:** 접두부가 만료되거나 재사용할 수 없으면 다시 처리해야 합니다. 제공자별 유휴 만료 규칙과 최신 OpenAI의 **최소** 보존 시간은 구분해야 합니다. 요청한 최소 시간이 지났다고 즉시 삭제가 보장되는 것은 아닙니다([Claude TTL](https://code.claude.com/docs/en/prompt-caching#cache-lifetime), [OpenAI 보존 의미](https://developers.openai.com/api/docs/guides/prompt-caching#cache-lifetime)).
3. **불필요한 압축:** Claude의 `/compact`는 요약 요청을 만들고 대화 계층을 바꾸지만, **시스템 프롬프트 캐시까지 항상 버리지는 않습니다.** 요약 입력의 캐시 상태와 남은 문맥에 따라 비용이 달라지며, 이 문서에서는 남은 턴 수에 따른 고정 손익분기점을 제시하지 않습니다. 이후 문맥 절감이 요약 비용을 정당화하는 자연스러운 경계에서 압축하고, 무관한 작업은 결정 사항을 기록한 뒤 `/clear`로 시작합니다([Claude 압축 동작](https://code.claude.com/docs/en/prompt-caching#compacting-the-conversation)).

Copilot에서는 모델·강도 변경을 원인으로 단정하기 전에 [Cache Explorer](https://code.visualstudio.com/docs/agents/agent-troubleshooting/cache-explorer)에서 첫 요청 차이와 보고된 재사용을 대조합니다. 요청 차이는 진단 근거이지 인과관계의 증명이나 청구 영수증이 아닙니다.

<a id="compaction-and-new-sessions"></a>
#### 제품별 압축·새 세션 제어

| 제품 | 연관된 작업을 계속할 때 | 무관한 작업으로 전환할 때 |
|---|---|---|
| **GitHub Copilot** | CLI는 자동 압축과 `/compact`를 지원하며 VS Code도 수동 압축을 제공합니다. CLI의 `/session checkpoints`로 저장된 요약을 확인할 수 있습니다([문맥 제어](https://docs.github.com/en/copilot/concepts/prompting-and-context/context-management)). | 결정 사항을 저장한 뒤 CLI `/new`·`/clear` 또는 새 VS Code 채팅을 사용합니다. 압축 요약에는 목표·핵심 상태가 남지만 과거의 모든 세부 내용이 남지는 않습니다([CLI 명령](https://docs.github.com/en/copilot/reference/copilot-cli-reference/cli-command-reference)). |
| **OpenAI Codex** | CLI `/compact`로 대화 기록을 줄입니다. 자동 압축 설정은 모델·클라이언트별로 확인하며 다른 제품의 임계값을 그대로 적용하지 않습니다([명령 참조](https://learn.chatgpt.com/docs/developer-commands?surface=cli), [설정 예시](https://learn.chatgpt.com/docs/config-file/config-sample)). | 필요한 인계 사항만 새 채팅·세션에 제공합니다. `codex fork`는 대안을 탐색하도록 기존 세션을 복사하는 것이지 빈 문맥을 만드는 기능은 아닙니다. |
| **Claude Code** | `/compact`는 대화를 요약하고 변경된 계층을 다시 구성하며, 재사용 가능한 안정적 계층은 캐시에 남을 수 있습니다([압축 계층](https://code.claude.com/docs/en/prompt-caching#compacting-the-conversation)). | 인계 사항을 저장하고 `/clear`를 사용합니다. 새 세션의 지침·사용량을 확인하며, 기록이 짧아졌다는 이유만으로 품질도 유지됐다고 판단하지 않습니다. |

<a id="4-subagents-vs-fork"></a>
### 4.4 위임·병렬 작업과 세션 분기

| 제품 | 위임한 문맥과 결과 | 사용량·포크·캐시 해석 |
|---|---|---|
| **GitHub Copilot** | CLI `/fleet`는 독립된 문맥의 서브에이전트를 조율합니다. VS Code는 하네스에 따라 다릅니다. Local의 `runSubagent`는 주 대화 기록을 상속하지 않지만 Copilot 하네스는 네이티브 `task`를 사용합니다. 작업마다 한정된 목표와 반환 결과를 명시합니다([fleet 동작](https://docs.github.com/en/copilot/concepts/agent-capabilities/choosing-and-delegating-agent-work/delegating-and-parallelizing-work-with-agents), [VS Code 차이](https://code.visualstudio.com/docs/agents/run/subagents)). | 별도의 모델 작업으로 AI Credits가 더 들 수 있습니다. CLI `/fork`로 대화 사본을 보존할 수 있어도 **제공자 캐시 재사용까지 보장되는 것은 아닙니다**([CLI 참조](https://docs.github.com/en/copilot/reference/copilot-cli-reference/cli-command-reference)). |
| **OpenAI Codex** | 로컬 클라이언트는 병렬 서브에이전트를 실행하고 요약을 모을 수 있습니다. 사용자 지정 에이전트가 모델·강도를 바꿀 수 있으므로 자식 설정도 기록합니다. 중간의 긴 출력은 주 대화 밖에 둡니다([서브에이전트](https://learn.chatgpt.com/docs/agent-configuration/subagents)). | OpenAI는 비교 가능한 단일 에이전트 실행보다 서브에이전트 작업에 더 많은 토큰이 든다고 안내합니다. `codex fork`는 기록을 복사하지만, 인용한 클라이언트 문서는 **모든 포크의 캐시 재사용을 보장하지 않습니다**([포크 명령](https://learn.chatgpt.com/docs/developer-commands?surface=cli)). |
| **Claude Code** | 서브에이전트는 별도 대화·시스템 프롬프트·도구로 시작하며, 별도 설정이 없으면 보통 `5분` TTL을 사용합니다. 부모는 모든 중간 작업 대신 결과를 받습니다([서브에이전트 캐싱](https://code.claude.com/docs/en/prompt-caching#subagents-and-the-cache)). | 자식의 첫 접두부는 부모와 다릅니다. 포크는 부모 프롬프트·도구·기록을 상속하므로 일치하는 캐시가 남아 있으면 재사용할 수 있지만 캐시 범위 조건도 충족해야 합니다. |

**격리가 무료 실행을 뜻하지는 않습니다.** 부모·자식 전체 작업량, 지연, 성공 여부를 비교하고 각 호출은 한 번만 집계합니다. 짧은 조회나 순차적 추적보다는 독립적이고 충분한 규모의 작업을 위임합니다([위임 손익분기 기준](../../copilot-tokenomics/03-context-hygiene/#subagent-delegation-break-even-rule)).

<a id="5-tokenizer-and-generation-inflation"></a>
### 4.5 모델 세대 교체와 토크나이저 인플레이션

모델이 바뀌면 토큰화 방식과 에이전트의 행동량이 모두 달라질 수 있습니다.
- **행동량은 작업에 따라 다릅니다:** VS Code 팀은 자체 데이터에서 세대별 작업당 토큰 증가를 관찰했다고 설명합니다. 이는 제공자의 관찰이지, 최신 모델이 언제나 더 많은 토큰을 쓴다는 법칙은 아닙니다([엔지니어링 보고서](https://code.visualstudio.com/blogs/2026/06/17/improving-token-efficiency-in-github-copilot)).
- **같은 입력을 다시 셉니다:** Anthropic은 Claude 4.7 이후 모델과 Mythos Preview의 새 토크나이저가 이전 모델보다 대략 **30% 더 많은 토큰**을 만들 수 있으며 콘텐츠·작업 구성에 따라 달라진다고 안내합니다. **제공자 설명이며 이 저장소에서 독립 재현한 수치는 아닙니다.** 모든 영문·한국어 프롬프트에 고정 배수를 적용하지 말고 대상 모델의 토큰 계수 도구와 실제 요청 사용량으로 확인합니다([공식 토큰 계수 안내](https://platform.claude.com/docs/en/build-with-claude/token-counting#supported-models), [언어와 토큰화](../../copilot-tokenomics/07-language-and-tokenization/)).

Copilot과 Codex는 **토크나이저가 아니라 제품**입니다. 하네스 차이를 보려면 각 제품에서 같은 모델·버전과 비교 가능한 강도를 선택해야 합니다. 불가능하면 결과를 **제품+모델 조합 비교**로 표시합니다. 사용자 프롬프트가 같아도 숨은 지침·도구·캐시 상태까지 같아지는 것은 아닙니다. [아래 통제 비교 예시](#controlled-comparison-exercise)에서는 이런 한계를 기록하며 제품별 고정 토큰 배수를 만들지 않습니다.

<a id="6-reasoning-and-output-weight"></a>
### 4.6 별도 단가가 적용되는 출력·추론 토큰

추론은 출력으로 과금되지만, 출력과 입력·캐시 읽기 사이에 모든 모델에 통하는 고정 배수는 없습니다. 예를 들어 GitHub의 GPT-6.1 Sol 기본 구간 요금은 100만 토큰당 입력 `$2`, 캐시 읽기 `$0.10`, 캐시 쓰기 `$2.50`, 출력 `$10`입니다. 다른 모델이나 긴 컨텍스트 구간에는 다른 요금이 적용됩니다([요금표](https://docs.github.com/en/copilot/reference/copilot-billing/models-and-pricing)).

추론 강도는 작업에 맞춰 선택한 뒤 비용과 성공 여부를 함께 검증합니다. 강도를 낮추면 토큰 사용을 줄일 수 있지만 품질까지 유지된다고 보장할 수는 없습니다([OpenAI 추론 강도 안내](https://developers.openai.com/api/docs/guides/reasoning#reasoning-effort)).

| 제품 | 모델·강도 선택과 확인 | 비용 해석 |
|---|---|---|
| **GitHub Copilot** | 클라이언트에서 사용할 수 있는 모델을 선택하며 CLI는 `--model`을 지원합니다. VS Code에서는 지원되는 추론 제어와 실제 요청을 확인하고 API 옵션이 화면에도 있다고 가정하지 않습니다([CLI 모델 참조](https://docs.github.com/en/copilot/reference/copilot-cli-reference/cli-command-reference), [Cache Explorer](https://code.visualstudio.com/docs/agents/agent-troubleshooting/cache-explorer)). | GitHub 요금표와 계정의 과금 방식을 적용합니다. SDK BYOK는 별도로 설정한 제공자의 계기판을 사용합니다. |
| **OpenAI Codex** | CLI `/model`로 사용 가능한 모델을 고르고 `/status`로 확인합니다. 추론 강도와 자식 에이전트의 재설정도 기록합니다([명령 참조](https://learn.chatgpt.com/docs/developer-commands?surface=cli), [서브에이전트 설정](https://learn.chatgpt.com/docs/agent-configuration/subagents)). | ChatGPT 제공량·크레딧과 API 키 요금을 분리합니다. 제공자의 추론 설정 업데이트가 모든 클라이언트에서 같은 방식으로 적용된다는 보장은 없습니다. |
| **Claude Code** | `/model`과 추론 강도 제어로 지원되는 설정을 고릅니다. 강도 변경의 캐시 유지 여부에는 모델·인증 예외가 적용됩니다([모델·강도 캐시 규칙](https://code.claude.com/docs/en/prompt-caching)). | API 출력 요금과 요금제 막대를 따로 해석합니다. 모델·강도를 바꿔 시험할 때 캐시 상태도 기록합니다. |

---

<a id="practical-playbook-and-five-metrics"></a>
## 5. 실전 절약 루틴과 공정한 5대 비교 지표

### 당장 적용할 수 있는 작업 지시·운영 루틴

| 상황 | 권장 루틴 | 피해야 할 패턴 |
|---|---|---|
| **버그 수정·기능 지시 시작** | **오류 메시지 + 관련 파일 경로 + 재현 명령**을 첫 프롬프트에 함께 제공해 초기 탐색 반경을 좁힘 | 파일 힌트 없이 *"로그인 안 되는 거 고쳐줘"*만 보내 전체 저장소 `Bash rg` 스캔 유도 |
| **코드 탐색 및 범위 읽기** | 정의·호출부를 범위와 함께 조회하며 [`cctx`](../../copilot-tokenomics/cctx/)는 선택 가능한 구현 중 하나 | 전체 파일 출력 또는 필요한 정의를 빠뜨리는 임의의 줄 분할 |
| **테스트·긴 로그 확인** | 테스트를 실행하고 상태·개수를 보존하되, 새 출력을 전달하기 전에 관련 실패를 추출 | 검증을 생략하거나 과거 도구 메시지를 축소해 덮어쓰기 |
| **작업 전환 vs 장기 작업 지속** | 새 세션 전 결정 사항을 기록하고, 계속할 작업은 이후 절감 효과가 있을 때 압축 | 남은 턴 수를 기준으로 모든 압축의 손익을 단정 |
| **모델·추론 강도 운용** | 작업에 맞는 설정으로 시작하고, 변경 전 캐시를 유지하는 업데이트 지원 여부 확인 | 모든 강도 변경이 무료이거나 모든 캐시를 무효화한다고 가정 |

<a id="product-operating-checklists"></a>
### 제품별 실전 체크리스트

| 제품 | 시작·범위 지정 | 문맥 증가·작업 전환 제어 | 결과 검증 |
|---|---|---|---|
| **GitHub Copilot** | 대상 파일·오류·재현 명령을 제공하고 모델과 VS Code 하네스를 고릅니다. CLI는 지침과 `/mcp`도 확인합니다. | CLI `/context`로 큰 항목을 찾고 새 로그 결과를 제한합니다. 연관 작업은 `/compact`, 무관한 작업은 `/new`를 사용합니다. VS Code는 CLI와 같다고 가정하지 말고 해당 화면의 제어를 사용합니다. | 테스트 후 CLI `/usage` 또는 VS Code Summary·Cache Explorer와 계정 AI Credits를 대조합니다. SDK는 실시간 이벤트와 누적 합계를 분리해 수집합니다. |
| **OpenAI Codex** | 로그인 방식과 `/status`를 확인하고 모델, 적용되는 `AGENTS.md` 체인, MCP 도구를 확인합니다. | 반복되는 전체 로그 대신 관련 테스트 실패만 전달합니다. 계속할 작업은 `/compact`, 무관한 작업은 새 채팅을 사용합니다. 포크에는 기존 문맥이 남습니다. | 테스트를 실행하고 자동화 시 `codex exec --json` 사용량을 보존합니다. ChatGPT 로그인은 계정 한도·크레딧, API 키는 API 청구와 대조합니다. |
| **Claude Code** | 모델, 로드한 `CLAUDE.md`·도구, 인증 방식, `/usage`를 확인합니다. | 필요한 정의는 완결된 범위로 읽되 출력을 제한합니다. 유용한 경계에서 `/compact`, 무관한 작업에는 인계 사항을 저장한 뒤 `/clear`를 사용합니다. | 테스트와 캐시 생성·읽기 사용량을 대조하고 API 달러 추정치와 구독 막대를 분리합니다. |

이 표는 앞의 [진단 경로](#usage-observability), [도구 제어](#2-standing-prompt-and-schema-tax), [세션 제어](#compaction-and-new-sessions)를 적용하는 체크리스트이며, 클라이언트 내부 구조가 같다는 근거는 아닙니다.

### 에이전트·도구 비교 시 기록할 5대 표준 지표

에이전트나 최적화 전후를 비교하기 전에 작업, 모델·클라이언트 버전, 인증·과금 방식, 캐시 설정을 기록합니다. 이후 [`tokprof`](../tokprof/) 등에서 확보할 수 있는 관측값으로 아래 **5개 묶음**을 나란히 비교합니다.

1. **신규 입력과 캐시 쓰기를 각각 기록:** 보존 시간·단가가 다르면 구분하고 하나의 가격으로 합치지 않습니다.
2. **캐시 읽기 토큰과 적중률:** 전체 입력 중 캐시에서 읽은 비율처럼 분모를 명시합니다.
3. **청구 출력과 보고된 추론 하위 내역:** 추론을 중복 합산하지 않습니다.
4. **실제 차감 비용·크레딧과 종단 간 시간:** API 추정 금액과 구독 사용량 관측값을 구분합니다.
5. **통과한 테스트·작업 성공 여부([제0원칙](../#제0-원칙)):** 실패한 시도까지 포함해 비용 옆에 품질을 기록합니다.

<a id="controlled-comparison-exercise"></a>
### 통제 비교 예시: 실측 벤치마크가 아닌 실행 절차

세 제품에 **아래 영문 프롬프트를 그대로** 사용합니다. 가능하면 같은 모델·버전과 비교 가능한 강도를 선택합니다. 클라이언트·하네스, 인증, 로드한 지침·도구, 보고된 캐시 상태를 기록합니다. 새 채팅이라고 제공자 캐시까지 비어 있다는 보장은 없습니다.

```text
Fix this Python function. Return only the corrected function.
Contract: return the arithmetic mean of a nonempty numeric list,
or None for an empty list. Do not use tools.

def mean(xs):
    return sum(xs) / len(xs)
```

격리된 임시 환경에서 반환된 함수에 같은 검사를 적용합니다.

```python
assert mean([]) is None
assert mean([2, 4]) == 3.0
assert mean([0]) == 0.0
```

| 제품 | 확인 가능한 유형별 토큰 | 실제 비용·크레딧과 소요 시간 | 통과 검사 / 전체 검사 |
|---|---|---|---|
| GitHub Copilot | 미측정 | 미측정 | 미실행 |
| OpenAI Codex | 미측정 | 미측정 | 미실행 |
| Claude Code | 미측정 | 미측정 | 미실행 |

이는 **합성 스모크 테스트 절차**이며 에이전트 호출이나 절감률을 실측했다는 주장이 아닙니다. 실제 관측값이 있을 때만 표를 채웁니다. 공개되지 않는 필드는 **확인 불가**로 표시하고 달러·제품 크레딧 단위를 분리하며 재시도·위임 호출도 포함합니다. 모델이 다르면 제품+모델 조합 비교입니다. 실제 작업에 대한 결론은 대표 고정 작업 세트를 반복 실행해 성공률과 비용을 함께 보고해야 하며, 위 검사만으로 코딩 품질을 평가할 수는 없습니다.

---

<a id="faq"></a>
## 6. 자주 묻는 질문

<div class="at-faq-item">
  <h3>총토큰 수가 4배 많으면 실제 비용이나 한도도 4배 더 드나요?</h3>
  <p>아닙니다. 비용은 토큰 유형과 적용 단가에 따라 달라지며 구독 한도 차감은 별도 관측값입니다. <a href="#three-agent-comparison">원문 요금표와 요금제 규칙</a>을 확인하고 실제 사용량을 작업 성공 여부와 함께 비교해야 합니다.</p>
</div>

<div class="at-faq-item">
  <h3>세션 중간에 /compact를 자주 실행하면 항상 토큰과 한도가 절약되나요?</h3>
  <p>아닙니다. Copilot·Codex·Claude Code의 압축·새 세션 제어는 서로 다릅니다. 요약과 바뀐 기록의 재구성에는 비용이 들지만 남겨 둔 문맥은 이후 작업을 줄일 수 있습니다. 고정된 턴 수 대신 <a href="#compaction-and-new-sessions">제품별 제어</a>를 사용해 사용량과 성공 여부를 함께 비교합니다.</p>
</div>

<div class="at-faq-item">
  <h3>추론 강도를 바꾸면 항상 프롬프트 캐시가 무효화되나요?</h3>
  <p>아닙니다. 모델·클라이언트·인증 경로에 따라 다릅니다. Copilot은 Cache Explorer로 보고된 재사용을 조사할 수 있습니다. 지원되는 OpenAI API와 Claude 모델에는 캐시를 유지하는 업데이트가 있지만 클라이언트가 이를 노출하지 않을 수도 있습니다. <a href="#3-silent-cache-invalidators">예외 조건과 공식 원문</a>을 확인합니다.</p>
</div>

<div class="at-faq-item">
  <h3>Copilot AI Credits를 모두 쓰면 모든 코딩 기능이 중단되나요?</h3>
  <p>아닙니다. 크레딧 소비 기능은 추가 사용 정책과 예산 제어를 따르지만, 인라인 완성과 다음 편집 제안은 공식 유료 요금제 규칙상 AI Credits 과금 대상이 아닙니다. <a href="https://docs.github.com/en/copilot/concepts/billing-and-usage/organizations-and-enterprises/billing">Copilot 과금·예산 동작</a>을 확인합니다.</p>
</div>

<div class="at-faq-item">
  <h3>Codex에서 API 키를 쓰면 ChatGPT 요금제 제공량이 늘어나나요?</h3>
  <p>아닙니다. API 키 세션은 지원되는 로컬 작업에 대해 Platform 계정으로 별도 과금됩니다. ChatGPT 사용량 한도를 충전하지 않으며 Codex cloud에는 ChatGPT 로그인이 필요합니다. <a href="https://learn.chatgpt.com/docs/auth">Codex 인증·과금 적용 범위</a>를 확인합니다.</p>
</div>

<a id="sources"></a>
## 출처

원문 확인일은 **2026-11-19**입니다. VS Code 엔지니어링 글은 **2026-06-17** 배포 당시의 설명이므로 모든 현재 클라이언트의 기본값이 같다는 근거로 사용하지 않습니다.

- **Copilot 정책·가격:** [개인 과금](https://docs.github.com/en/copilot/concepts/billing-and-usage/individuals/billing), [조직 과금](https://docs.github.com/en/copilot/concepts/billing-and-usage/organizations-and-enterprises/billing), [모델별 단가](https://docs.github.com/en/copilot/reference/copilot-billing/models-and-pricing), [기존 요청 과금](https://docs.github.com/en/copilot/reference/copilot-billing/request-based-billing-legacy/copilot-requests).
- **Copilot 작업 흐름:** [CLI 명령](https://docs.github.com/en/copilot/reference/copilot-cli-reference/cli-command-reference), [문맥·체크포인트](https://docs.github.com/en/copilot/concepts/prompting-and-context/context-management), [MCP 설정](https://docs.github.com/en/copilot/how-tos/copilot-cli/customize-copilot/add-mcp-servers), [위임](https://docs.github.com/en/copilot/concepts/agent-capabilities/choosing-and-delegating-agent-work/delegating-and-parallelizing-work-with-agents).
- **Copilot SDK·VS Code 진단:** [SDK 사용량 지표](https://docs.github.com/en/copilot/how-tos/copilot-sdk/features/usage-and-billing), [SDK BYOK](https://docs.github.com/en/copilot/how-tos/copilot-sdk/auth/byok), [디버그 화면](https://code.visualstudio.com/docs/agents/agent-troubleshooting/chat-debug-view), [Cache Explorer](https://code.visualstudio.com/docs/agents/agent-troubleshooting/cache-explorer), [하네스별 서브에이전트](https://code.visualstudio.com/docs/agents/run/subagents).
- **Codex 제품 동작:** [요금제·크레딧](https://learn.chatgpt.com/docs/pricing), [인증](https://learn.chatgpt.com/docs/auth), [개발자 명령](https://learn.chatgpt.com/docs/developer-commands?surface=cli), [JSONL 사용량 이벤트](https://learn.chatgpt.com/docs/non-interactive-mode), [서브에이전트](https://learn.chatgpt.com/docs/agent-configuration/subagents).
- **Codex 설정·OpenAI API:** [지침](https://learn.chatgpt.com/docs/agent-configuration/agents-md), [MCP 제어](https://learn.chatgpt.com/docs/extend/mcp?surface=cli), [설정 예시](https://learn.chatgpt.com/docs/config-file/config-sample), [API 프롬프트 캐싱](https://developers.openai.com/api/docs/guides/prompt-caching), [추론 사용량](https://developers.openai.com/api/docs/guides/reasoning), [접두부를 유지하는 업데이트](https://developers.openai.com/api/docs/guides/keep-prefix-unchanged).
- **Claude 제품 정책:** [요금제와 가격](https://claude.com/pricing), [사용량과 API 비용 추정](https://code.claude.com/docs/en/costs).
- **Claude 캐시·토큰 정의:** [Claude Code 캐싱](https://code.claude.com/docs/en/prompt-caching), [API 캐시 요금](https://platform.claude.com/docs/en/build-with-claude/prompt-caching), [사고 토큰 과금](https://platform.claude.com/docs/en/build-with-claude/thinking), [토큰 계수](https://platform.claude.com/docs/en/build-with-claude/token-counting).
- **VS Code 엔지니어링 자료(제공자 보고이며 독립 재현하지 않음):** [Improving token efficiency for GitHub Copilot in VS Code](https://code.visualstudio.com/blogs/2026/06/17/improving-token-efficiency-in-github-copilot).
