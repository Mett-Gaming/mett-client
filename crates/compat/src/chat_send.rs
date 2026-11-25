//! Findet Minecrafts Sendefunktionen über den Namen des Senden-Knopfs. Jede Stufe muss genau einen Treffer haben.
use crate::image::{Image, name_id};
use iced_x86::{Code, Decoder, DecoderOptions, Instruction, Mnemonic, OpKind, Register};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Send {
    /// (Modell, Text)
    pub chat: usize,
    /// (Text, gezählter Zeiger auf die Spielinstanz). `None`, wenn die Übergabe anders aussieht als bekannt;
    /// dann gehen nur Nachrichten.
    pub command: Option<usize>,
    /// Abstand des Modells im Controller, aus dem Sende-Code gelesen.
    pub model_offset: usize,
}

/// Stufe, an der die Suche endete, mit verständlichem Grund.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Missing {
    Button,
    Table,
    Handler,
    Controller,
    Slash,
    Calls,
    Model,
}

impl Missing {
    pub fn text(self) -> &'static str {
        match self {
            Missing::Button => "Senden-Knopf nicht eindeutig gefunden",
            Missing::Table => "Funktionstabelle des Knopfs nicht eindeutig",
            Missing::Handler => "Funktion des Knopfs nicht erkannt",
            Missing::Controller => "Senden des Chatfensters nicht erkannt",
            Missing::Slash => "Prüfung auf / nicht eindeutig",
            Missing::Calls => "Sendeaufrufe nicht eindeutig",
            Missing::Model => "Lage des Modells nicht erkannt",
        }
    }
}

fn one<T: Copy>(v: &[T], e: Missing) -> Result<T, Missing> {
    match v {
        [x] => Ok(*x),
        _ => Err(e),
    }
}

fn decode(i: &Image, start: usize, end: usize) -> Vec<Instruction> {
    let Some(code) = i.code(start..end) else { return vec![] };
    Decoder::with_ip(64, code, start as u64, DecoderOptions::NONE).into_iter().collect()
}

fn direct_call(x: &Instruction) -> Option<usize> {
    (x.code() == Code::Call_rel32_64).then(|| x.near_branch64() as usize)
}

pub fn find(i: &Image) -> Result<Send, Missing> {
    // `mov edx, Kennung` bei der Anmeldung des Knopfs.
    let mut pattern = vec![0xBA];
    pattern.extend(name_id("button.send").to_le_bytes());
    let at = one(&i.find(&pattern, &i.text), Missing::Button)?;

    // Kurz davor wird die Funktionstabelle geladen: lea rax, [rip+x]. Anfang unbekannt, daher nach Bytes gesucht.
    let before = at.saturating_sub(0x30)..at;
    let tables: Vec<usize> = i
        .find(&[0x48, 0x8D, 0x05], &before)
        .into_iter()
        .filter_map(|l| Some((l as i64 + 7 + i.u32(l + 3)? as i32 as i64) as usize))
        .filter(|t| i.rdata.contains(t) && t % 8 == 0)
        .collect();
    let table = one(&tables, Missing::Table)?;

    let handler = i.pointer(table + 16).ok_or(Missing::Handler)?;
    let h = i.function(handler).ok_or(Missing::Handler)?;
    let controller = decode(i, h.start, h.end).iter().find_map(direct_call).ok_or(Missing::Controller)?;
    let c = i.function(controller).ok_or(Missing::Controller)?;

    let code = decode(i, c.start, c.end);
    let slash: Vec<usize> = (0..code.len())
        .filter(|k| {
            let x = &code[*k];
            x.mnemonic() == Mnemonic::Cmp
                && x.op0_kind() == OpKind::Memory
                && x.memory_base() == Register::RAX
                && x.memory_displacement64() == 0
                && x.op1_kind() == OpKind::Immediate8
                && x.immediate8() == b'/'
        })
        .collect();
    let slash = one(&slash, Missing::Slash)?;
    // Direkt vor der Prüfung lädt der Code das Modell: mov rcx, [Controller+x].
    let load = slash.checked_sub(1).map(|k| &code[k]).ok_or(Missing::Model)?;
    let loads_model = load.mnemonic() == Mnemonic::Mov
        && load.op0_kind() == OpKind::Register
        && load.op0_register() == Register::RCX
        && load.op1_kind() == OpKind::Memory
        && load.memory_index() == Register::None
        && load.memory_base() != Register::RIP;
    let model_offset = load.memory_displacement64() as usize;
    if !loads_model || !model_offset.is_multiple_of(8) || model_offset == 0 || model_offset > 0x10000 {
        return Err(Missing::Model);
    }
    // Nur die beiden Sendeaufrufe melden zurück, ob der Chat eingeschränkt ist (`cmp al, 1`).
    let calls: Vec<(usize, usize)> = (slash..code.len() - 1)
        .filter(|k| code[k + 1].code() == Code::Cmp_AL_imm8 && code[k + 1].immediate8() == 1)
        .filter_map(|k| Some((k, direct_call(&code[k])?)))
        .collect();
    match calls[..] {
        [(at, command), (_, chat)]
            if command != chat && i.function(command).is_some() && i.function(chat).is_some() =>
        {
            Ok(Send { chat, command: passes_owner(&code[slash..at]).then_some(command), model_offset })
        }
        _ => Err(Missing::Calls),
    }
}

/// Vor dem Befehl kopiert der Code einen gezählten Zeiger: Lebendzeichen bei +0x48, Zähler bei +0x50 (erhöht bei +8),
/// Instanz bei +0x58. Genau so übergibt ihn das Plugin; jede andere Form gilt als unbekannt.
fn passes_owner(code: &[Instruction]) -> bool {
    let loads = |offset: u64| {
        code.iter().any(|x| {
            x.mnemonic() == Mnemonic::Mov
                && x.op0_kind() == OpKind::Register
                && x.op1_kind() == OpKind::Memory
                && x.memory_base() != Register::RIP
                && x.memory_displacement64() == offset
        })
    };
    let counts = code.iter().filter(|x| x.has_lock_prefix()).collect::<Vec<_>>();
    let counted = matches!(counts[..], [x] if x.mnemonic() == Mnemonic::Inc && x.memory_displacement64() == 8
        && x.memory_size().size() == 4);
    loads(0x48) && loads(0x50) && loads(0x58) && counted
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::image::tests::{BASE, blank, put};

    fn call(b: &mut [u8], at: usize, to: usize) {
        b[at] = 0xE8;
        put(b, at + 1, &((to as i64 - at as i64 - 5) as i32).to_le_bytes());
    }

    /// Nachgebaute Kette wie im Spiel, um `shift` verschoben.
    fn game(shift: usize) -> Vec<u8> {
        let mut b = blank();
        let t = 0x1000 + shift;
        let (handler, controller, command, chat) = (t + 0x100, t + 0x200, t + 0x300, t + 0x340);
        let table = 0x2100 + shift;
        put(&mut b, t, &[0x48, 0x8D, 0x05]);
        put(&mut b, t + 3, &((table - t - 7) as u32).to_le_bytes());
        b[t + 7] = 0xBA;
        put(&mut b, t + 8, &name_id("button.send").to_le_bytes());
        put(&mut b, table + 16, &(BASE + handler as u64).to_le_bytes());
        call(&mut b, handler, controller);
        // mov rcx, [rsi+0xd48]; cmp byte ptr [rax], '/'
        put(&mut b, controller, &[0x48, 0x8B, 0x8E, 0x48, 0x0D, 0, 0, 0x80, 0x38, b'/']);
        // mov rcx,[rax+0x50]; lock inc dword ptr [rcx+8]; mov rcx,[rax+0x48]; mov rax,[rax+0x58]
        let owner = [0x48, 0x8B, 0x48, 0x50, 0xF0, 0xFF, 0x41, 8, 0x48, 0x8B, 0x48, 0x48, 0x48, 0x8B, 0x40, 0x58];
        put(&mut b, controller + 10, &owner);
        for (k, to) in [command, chat].into_iter().enumerate() {
            call(&mut b, controller + 26 + k * 7, to);
            put(&mut b, controller + 31 + k * 7, &[0x3C, 1]);
        }
        for (k, s) in [handler, controller, command, chat].into_iter().enumerate() {
            put(&mut b, 0x4000 + k * 12, &(s as u32).to_le_bytes());
            put(&mut b, 0x4004 + k * 12, &(s as u32 + 0x40).to_le_bytes());
        }
        b
    }

    fn run(b: &[u8]) -> Result<Send, Missing> {
        find(&Image::new(BASE, b).unwrap())
    }

    #[test]
    fn findet_die_funktionen_auch_nach_verschiebung() {
        for shift in [0, 0x80, 0x400] {
            assert_eq!(
                run(&game(shift)),
                Ok(Send { command: Some(0x1300 + shift), chat: 0x1340 + shift, model_offset: 0xD48 })
            );
        }
    }

    #[test]
    fn andere_uebergabe_kostet_nur_die_befehle() {
        // Zähler an anderer Stelle, kein Zählen, anderer Feldabstand.
        for (at, byte) in [(0x1211, 0x10u8), (0x120E, 0x90), (0x1219, 0x60)] {
            let mut b = game(0);
            b[at] = byte;
            let s = run(&b).unwrap();
            assert_eq!((s.command, s.chat), (None, 0x1340), "{at:x}");
        }
    }

    #[test]
    fn veraendertes_spiel_schaltet_ab_statt_zu_raten() {
        let mut doppelt = game(0);
        let button: Vec<u8> = doppelt[0x1007..0x100c].to_vec();
        put(&mut doppelt, 0x1800, &button);
        assert_eq!(run(&doppelt), Err(Missing::Button));

        let mut ohne = game(0);
        put(&mut ohne, 0x1008, &[0; 4]);
        assert_eq!(run(&ohne), Err(Missing::Button));

        let mut tabelle = game(0);
        put(&mut tabelle, 0x1000, &[0x90; 3]);
        assert_eq!(run(&tabelle), Err(Missing::Table));

        let mut handler = game(0);
        put(&mut handler, 0x2110, &[0; 8]);
        assert_eq!(run(&handler), Err(Missing::Handler));

        let mut slash = game(0);
        slash[0x1209] = b'!';
        assert_eq!(run(&slash), Err(Missing::Slash));

        let mut calls = game(0);
        put(&mut calls, 0x1226, &[0x90; 2]);
        assert_eq!(run(&calls), Err(Missing::Calls));

        let mut model = game(0);
        put(&mut model, 0x1200, &[0x90; 7]);
        assert_eq!(run(&model), Err(Missing::Model));
    }
}
