"""Cross-session trend analysis with moving averages."""
from collections import deque
from dataclasses import dataclass, field
from typing import Optional
import statistics

@dataclass
class SessionMetrics:
    session_id: str
    hr_mean: float
    br_mean: float
    anomaly_count: int
    risk_score: float

class TrendAnalyzer:
    def __init__(self, window: int = 10):
        self._history: deque[SessionMetrics] = deque(maxlen=window)

    def add(self, m: SessionMetrics) -> None:
        self._history.append(m)

    def analyze(self, current: SessionMetrics) -> dict:
        if len(self._history) < 2:
            return {"trend": "insufficient_data", "sessions": len(self._history),
                    "changes": [], "moving_avg_hr": current.hr_mean,
                    "moving_avg_br": current.br_mean}
        hrs = [s.hr_mean for s in self._history]
        brs = [s.br_mean for s in self._history]
        risks = [s.risk_score for s in self._history]
        hr_ma = statistics.mean(hrs[-5:])
        br_ma = statistics.mean(brs[-5:])
        hr_slope = (hrs[-1] - hrs[0]) / len(hrs)
        br_slope = (brs[-1] - brs[0]) / len(brs)
        risk_slope = (risks[-1] - risks[0]) / len(risks) if len(risks) > 1 else 0
        changes = []
        if abs(hr_slope) > 2:
            changes.append(f"HR trending {'up' if hr_slope > 0 else 'down'} {abs(hr_slope):.1f} bpm/session")
        if abs(br_slope) > 0.5:
            changes.append(f"BR trending {'up' if br_slope > 0 else 'down'} {abs(br_slope):.1f} rpm/session")
        if risk_slope > 3:
            changes.append(f"Risk score increasing {risk_slope:.1f} pts/session")
        trend = ("deteriorating" if risk_slope > 5 or hr_slope > 5 else
                 "improving" if risk_slope < -5 else "stable")
        return {
            "trend": trend, "sessions": len(self._history),
            "moving_avg_hr": round(hr_ma, 1), "moving_avg_br": round(br_ma, 1),
            "hr_slope": round(hr_slope, 2), "br_slope": round(br_slope, 2),
            "risk_slope": round(risk_slope, 2), "changes": changes,
        }

_analyzer = TrendAnalyzer(window=20)
def get_analyzer() -> TrendAnalyzer:
    return _analyzer
