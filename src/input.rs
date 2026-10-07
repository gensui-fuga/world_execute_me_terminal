//! 键盘输入到语义动作的映射。
//!
//! 与渲染解耦：`main` 只消费 `Action`，不关心 crossterm 细节。
//! 所有轮询都是非阻塞的，主循环每帧把事件抽干，避免输入积压。

use std::time::Duration;

use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};

/// 一个语义动作。
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Action {
    /// q / Esc / Ctrl-C
    Quit,
    /// 空格
    TogglePause,
    /// ← / →
    Seek(f64),
    /// [ / ] —— 微调音画偏移
    Offset(f64),
    /// m —— 循环切换渲染模式
    CycleMode,
    /// d —— 调试面板
    ToggleDebug,
    /// r —— 重新开始
    Restart,
}

/// 非阻塞地取走当前所有待处理动作。
pub fn poll_actions() -> Result<Vec<Action>> {
    let mut out = Vec::new();
    while event::poll(Duration::from_millis(0))? {
        if let Event::Key(k) = event::read()? {
            if k.kind != KeyEventKind::Press {
                continue;
            }
            if let Some(a) = translate(k.code, k.modifiers) {
                out.push(a);
            }
        }
    }
    Ok(out)
}

/// 单键映射。返回 `None` 表示未绑定。
pub fn translate(code: KeyCode, mods: KeyModifiers) -> Option<Action> {
    match (code, mods) {
        (KeyCode::Char('q'), _) | (KeyCode::Esc, _) => Some(Action::Quit),
        (KeyCode::Char('c'), KeyModifiers::CONTROL) => Some(Action::Quit),
        (KeyCode::Char(' '), _) => Some(Action::TogglePause),
        (KeyCode::Left, _) => Some(Action::Seek(-5.0)),
        (KeyCode::Right, _) => Some(Action::Seek(5.0)),
        (KeyCode::Char('['), _) => Some(Action::Offset(-0.05)),
        (KeyCode::Char(']'), _) => Some(Action::Offset(0.05)),
        (KeyCode::Char('m'), _) => Some(Action::CycleMode),
        (KeyCode::Char('d'), _) => Some(Action::ToggleDebug),
        (KeyCode::Char('r'), _) => Some(Action::Restart),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quit_keys_are_mapped() {
        assert_eq!(
            translate(KeyCode::Char('q'), KeyModifiers::NONE),
            Some(Action::Quit)
        );
        assert_eq!(
            translate(KeyCode::Esc, KeyModifiers::NONE),
            Some(Action::Quit)
        );
        assert_eq!(
            translate(KeyCode::Char('c'), KeyModifiers::CONTROL),
            Some(Action::Quit)
        );
    }

    #[test]
    fn seek_directions_are_signed() {
        assert_eq!(
            translate(KeyCode::Left, KeyModifiers::NONE),
            Some(Action::Seek(-5.0))
        );
        assert_eq!(
            translate(KeyCode::Right, KeyModifiers::NONE),
            Some(Action::Seek(5.0))
        );
    }

    #[test]
    fn unbound_key_is_none() {
        assert_eq!(translate(KeyCode::F(9), KeyModifiers::NONE), None);
    }
}
