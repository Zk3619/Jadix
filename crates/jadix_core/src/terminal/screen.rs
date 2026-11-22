//! Streaming VT parser and bounded screen shared by GUI terminal adapters.
use super::TerminalSpec;
use serde::Serialize;
use std::collections::VecDeque;
use unicode_width::UnicodeWidthChar;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct CellStyle { pub foreground: Option<u32>, pub background: Option<u32>, pub bold: bool, pub italic: bool, pub underline: bool, pub inverse: bool }
#[derive(Clone, Default)]
struct Cell { text: String, style: CellStyle, continuation: bool }
#[derive(Serialize)]
pub struct TerminalRun { pub text: String, pub style: CellStyle }
#[derive(Serialize)]
pub struct TerminalFrame { pub runs: Vec<TerminalRun>, pub cursor: [usize; 2], pub cursor_utf16: usize, pub cursor_visible: bool, pub title: String, pub replies: Vec<String>, pub application_cursor: bool, pub bracketed_paste: bool }
struct Screen {
    spec: TerminalSpec, cells: Vec<Vec<Cell>>, history: VecDeque<Vec<Cell>>,
    alternate: Option<Vec<Vec<Cell>>>, cursor: [usize; 2], saved: [usize; 2],
    region: [usize; 2], style: CellStyle, wrap: bool, wrap_pending: bool,
    cursor_visible: bool, application_cursor: bool, bracketed_paste: bool,
    title: String, replies: Vec<String>,
}
pub struct TerminalScreen { parser: vte::Parser, screen: Screen }
impl TerminalScreen {
    pub fn new(spec: TerminalSpec) -> Result<Self, &'static str> {
        spec.validate()?; let columns = spec.columns as usize; let rows = spec.rows as usize;
        Ok(Self { parser: vte::Parser::new(), screen: Screen { spec, cells: vec![vec![Cell::default(); columns]; rows], history: VecDeque::new(), alternate: None, cursor: [0,0], saved:[0,0], region:[0,rows-1], style:CellStyle::default(), wrap:true, wrap_pending:false, cursor_visible:true, application_cursor:false, bracketed_paste:false, title:String::new(), replies:vec![] } })
    }
    pub fn feed(&mut self, bytes: &[u8]) -> Result<(), &'static str> {
        if bytes.len() > 64 * 1024 { return Err("terminal chunk exceeds budget"); }
        self.parser.advance(&mut self.screen, bytes); Ok(())
    }
    pub fn resize(&mut self, columns: u16, rows: u16) -> Result<(), &'static str> {
        self.configure(TerminalSpec { columns, rows, scrollback: self.screen.spec.scrollback })
    }
    /// Change screen dimensions and retained history atomically, preserving the VT parser.
    pub fn configure(&mut self, spec: TerminalSpec) -> Result<(), &'static str> {
        spec.validate()?;
        let (columns, rows) = (spec.columns, spec.rows);
        let screen = &mut self.screen;
        let dimensions_changed = (screen.spec.columns, screen.spec.rows) != (columns, rows);
        screen.spec = spec;
        while screen.history.len() > screen.spec.scrollback { screen.history.pop_front(); }
        if !dimensions_changed { return Ok(()); }
        for line in screen.cells.iter_mut().chain(screen.history.iter_mut()).chain(screen.alternate.iter_mut().flatten()) { line.resize(columns as usize, Cell::default()); }
        screen.cells.resize(rows as usize, vec![Cell::default(); columns as usize]);
        if let Some(alt) = &mut screen.alternate { alt.resize(rows as usize, vec![Cell::default(); columns as usize]); }
        screen.cursor[0] = screen.cursor[0].min(columns as usize-1); screen.cursor[1] = screen.cursor[1].min(rows as usize-1);
        screen.saved[0] = screen.saved[0].min(columns as usize-1); screen.saved[1] = screen.saved[1].min(rows as usize-1);
        screen.region = [0,rows as usize-1]; screen.wrap_pending = false; Ok(())
    }
    pub fn frame(&mut self) -> TerminalFrame {
        let screen = &mut self.screen; let mut runs: Vec<TerminalRun> = vec![];
        let history_len = if screen.alternate.is_none() { screen.history.len() } else { 0 };
        let mut cursor_utf16 = 0; let mut offset = 0;
        for (index,line) in screen.history.iter().take(history_len).chain(screen.cells.iter()).enumerate() {
            let end = line.iter().rposition(|c| !c.text.is_empty() || c.style.background.is_some()).map_or(0, |i| i+1);
            let cursor_line = index == history_len + screen.cursor[1];
            let end = if cursor_line { end.max(screen.cursor[0]) } else { end };
            for (x,cell) in line[..end].iter().enumerate() {
                if cursor_line && x == screen.cursor[0] { cursor_utf16 = offset; }
                if !cell.continuation { let text = if cell.text.is_empty() { " " } else { &cell.text }; append(&mut runs, text, &cell.style); offset += text.encode_utf16().count(); }
            }
            if cursor_line && screen.cursor[0] >= end { cursor_utf16 = offset; }
            if index+1 < history_len + screen.cells.len() { append(&mut runs,"\n",&CellStyle::default()); offset += 1; }
        }
        TerminalFrame { runs, cursor:[screen.cursor[0],screen.cursor[1]+history_len], cursor_utf16, cursor_visible:screen.cursor_visible, title:screen.title.clone(), replies:std::mem::take(&mut screen.replies), application_cursor:screen.application_cursor, bracketed_paste:screen.bracketed_paste }
    }
}
fn append(runs:&mut Vec<TerminalRun>, text:&str, style:&CellStyle) { if let Some(run)=runs.last_mut().filter(|r| &r.style==style) {run.text.push_str(text)}else{runs.push(TerminalRun{text:text.into(),style:style.clone()})} }
impl Screen {
    fn blank(&self) -> Cell { Cell { style:self.style.clone(), ..Cell::default() } }
    fn scroll(&mut self, up:bool, count:usize) {
        for _ in 0..count.min(self.region[1]-self.region[0]+1) {
            let blank=vec![self.blank();self.spec.columns as usize];
            if up { let line=self.cells.remove(self.region[0]); self.cells.insert(self.region[1],blank);
                if self.region==[0,self.cells.len()-1] && self.alternate.is_none() { self.history.push_back(line); while self.history.len()>self.spec.scrollback {self.history.pop_front();} }
            } else {self.cells.remove(self.region[1]);self.cells.insert(self.region[0],blank);}
        }
    }
    fn newline(&mut self) { self.wrap_pending=false; if self.cursor[1]==self.region[1] {self.scroll(true,1)}else{self.cursor[1]=(self.cursor[1]+1).min(self.cells.len()-1)} }
    fn erase(&mut self, start:[usize;2], end:[usize;2]) { let blank=self.blank(); for y in start[1]..=end[1] { let first=if y==start[1]{start[0]}else{0};let last=if y==end[1]{end[0]}else{self.spec.columns as usize-1}; self.cells[y][first..=last].fill(blank.clone()); } }
    fn sgr(&mut self, params:&[usize]) {
        let mut i=0; while i<params.len() { let p=params[i]; match p {
            0=>self.style=CellStyle::default(),1=>self.style.bold=true,3=>self.style.italic=true,4=>self.style.underline=true,7=>self.style.inverse=true,
            22=>self.style.bold=false,23=>self.style.italic=false,24=>self.style.underline=false,27=>self.style.inverse=false,
            30..=37|90..=97=>self.style.foreground=Some(color(if p>=90{p-90+8}else{p-30})),
            40..=47|100..=107=>self.style.background=Some(color(if p>=100{p-100+8}else{p-40})),39=>self.style.foreground=None,49=>self.style.background=None,
            38|48=>{let c=if params.get(i+1)==Some(&5)&&params.len()>i+2 {i+=2;Some(color(params[i]))}else if params.get(i+1)==Some(&2)&&params.len()>i+4 {i+=4;Some(((params[i-2].min(255) as u32)<<16)|((params[i-1].min(255) as u32)<<8)|params[i].min(255) as u32)}else{None};if p==38{self.style.foreground=c}else{self.style.background=c}},_=>{}
        } i+=1; }
    }
}
fn color(index:usize)->u32 { const COLORS:[u32;16]=[0x1d1f21,0xcc6666,0xb5bd68,0xf0c674,0x81a2be,0xb294bb,0x8abeb7,0xc5c8c6,0x666666,0xd54e53,0xb9ca4a,0xe7c547,0x7aa6da,0xc397d8,0x70c0b1,0xffffff]; match index {0..=15=>COLORS[index],16..=231=>{let n=index-16;let f=|v|if v==0{0}else{55+v*40};((f(n/36) as u32)<<16)|((f(n/6%6) as u32)<<8)|f(n%6) as u32},232..=255=>{let v=(8+(index-232)*10)as u32;(v<<16)|(v<<8)|v},_=>0} }
impl vte::Perform for Screen {
    fn print(&mut self,c:char) {
        let width=c.width().unwrap_or(0);
        if width==0 {let x=self.cursor[0].saturating_sub(usize::from(!self.wrap_pending));let cell=&mut self.cells[self.cursor[1]][x];if cell.text.len()<256{cell.text.push(c);}return}
        if self.wrap_pending || (width==2 && self.cursor[0]+1>=self.spec.columns as usize) {if self.wrap {self.cursor[0]=0;self.newline();}self.wrap_pending=false;}
        let [x,y]=self.cursor; let columns=self.spec.columns as usize;
        if self.cells[y][x].continuation && x>0 {self.cells[y][x-1]=self.blank();}
        if x+1<columns && self.cells[y][x+1].continuation {self.cells[y][x+1]=self.blank();}
        self.cells[y][x]=Cell{text:c.to_string(),style:self.style.clone(),continuation:false};
        if width==2 && x+1<columns {self.cells[y][x+1]=Cell{continuation:true,..self.blank()};}
        if x+width>=columns {self.cursor[0]=columns-1;self.wrap_pending=self.wrap}else{self.cursor[0]+=width;}
    }
    fn execute(&mut self, byte:u8) {match byte{b'\r'=>{self.cursor[0]=0;self.wrap_pending=false},b'\n'|0x0b|0x0c=>self.newline(),8=>{self.cursor[0]=self.cursor[0].saturating_sub(1);self.wrap_pending=false},9=>{self.cursor[0]=((self.cursor[0]/8+1)*8).min(self.spec.columns as usize-1);self.wrap_pending=false},_=>{}}}
    fn osc_dispatch(&mut self, params:&[&[u8]],_:bool) { if params.len()>1 && [b"0".as_slice(),b"2".as_slice()].contains(&params[0]) {self.title=String::from_utf8_lossy(params[1]).chars().filter(|c|!c.is_control()).take(512).collect();} }
    fn esc_dispatch(&mut self,intermediates:&[u8],ignore:bool,byte:u8) {if ignore||!intermediates.is_empty(){return}match byte {b'7'=>self.saved=self.cursor,b'8'=>{self.cursor=self.saved;self.wrap_pending=false},b'D'=>self.newline(),b'E'=>{self.cursor[0]=0;self.newline()},b'M'=>{if self.cursor[1]==self.region[0]{self.scroll(false,1)}else{self.cursor[1]=self.cursor[1].saturating_sub(1)}},b'c'=>{self.style=CellStyle::default();self.cursor=[0,0];self.erase([0,0],[self.spec.columns as usize-1,self.cells.len()-1]);},_=>{}}}
    fn csi_dispatch(&mut self,params:&vte::Params,intermediates:&[u8],ignore:bool,action:char) {
        if ignore{return}let p:Vec<usize>=params.iter().flat_map(|p|p.iter().map(|v|*v as usize)).collect(); let first=p.first().copied().unwrap_or(0);let n=first.max(1);let columns=self.spec.columns as usize;let rows=self.cells.len();
        if intermediates==b"?" {if action=='h'||action=='l'{let enabled=action=='h';for value in p{match value{1=>self.application_cursor=enabled,7=>self.wrap=enabled,25=>self.cursor_visible=enabled,2004=>self.bracketed_paste=enabled,1049|47|1047=>{if enabled&&self.alternate.is_none(){self.saved=self.cursor;self.alternate=Some(std::mem::replace(&mut self.cells,vec![vec![Cell::default();columns];rows]));self.cursor=[0,0];}else if !enabled{if let Some(cells)=self.alternate.take(){self.cells=cells;self.cursor=self.saved;}}self.region=[0,rows-1];},_=>{}}}}return}
        if !intermediates.is_empty(){return}match action {
            'A'=>self.cursor[1]=self.cursor[1].saturating_sub(n),'B'|'e'=>self.cursor[1]=(self.cursor[1]+n).min(rows-1),'C'|'a'=>self.cursor[0]=(self.cursor[0]+n).min(columns-1),'D'=>self.cursor[0]=self.cursor[0].saturating_sub(n),
            'E'=>{self.cursor[1]=(self.cursor[1]+n).min(rows-1);self.cursor[0]=0},'F'=>{self.cursor[1]=self.cursor[1].saturating_sub(n);self.cursor[0]=0},'G'|'`'=>self.cursor[0]=(n-1).min(columns-1),'d'=>self.cursor[1]=(n-1).min(rows-1),
            'H'|'f'=>self.cursor=[p.get(1).copied().unwrap_or(1).max(1).min(columns)-1,n.min(rows)-1],
            'J'=>match first{0=>self.erase(self.cursor,[columns-1,rows-1]),1=>self.erase([0,0],self.cursor),2|3=>{self.erase([0,0],[columns-1,rows-1]);if first==3{self.history.clear()}},_=>{}},
            'K'=>match first{0=>self.erase(self.cursor,[columns-1,self.cursor[1]]),1=>self.erase([0,self.cursor[1]],self.cursor),2=>self.erase([0,self.cursor[1]],[columns-1,self.cursor[1]]),_=>{}},
            'X'=>self.erase(self.cursor,[(self.cursor[0]+n-1).min(columns-1),self.cursor[1]]),
            'P'|'@'=>{let blank=self.blank();let x=self.cursor[0];let line=&mut self.cells[self.cursor[1]];for _ in 0..n.min(columns-x){if action=='P'{line.remove(x);line.push(blank.clone());}else{line.pop();line.insert(x,blank.clone());}}},
            'S'=>self.scroll(true,n),'T'=>self.scroll(false,n),
            'L'|'M'=>{if (self.region[0]..=self.region[1]).contains(&self.cursor[1]){let top=self.region[0];self.region[0]=self.cursor[1];self.scroll(action=='M',n);self.region[0]=top;}},
            'r'=>{let end=p.get(1).copied().unwrap_or(rows).max(1).min(rows)-1;if n-1<end{self.region=[n-1,end];self.cursor=[0,0];}},
            's'=>self.saved=self.cursor,'u'=>self.cursor=self.saved,'m'=>self.sgr(&p),
            'n' if first==6=>{if self.replies.len()<64{self.replies.push(format!("\x1b[{};{}R",self.cursor[1]+1,self.cursor[0]+1));}},
            'n' if first==5=>{if self.replies.len()<64{self.replies.push("\x1b[0n".into());}},'c'=>{if self.replies.len()<64{self.replies.push("\x1b[?1;2c".into());}},_=>{}
        } if action!='m'{self.wrap_pending=false}
    }
}
#[cfg(test)] mod tests {
    use super::*;
    fn terminal()->TerminalScreen{TerminalScreen::new(TerminalSpec{columns:4,rows:2,scrollback:2}).unwrap()}
    fn text(t:&mut TerminalScreen)->String{t.frame().runs.into_iter().map(|r|r.text).collect()}
    #[test] fn split_utf8_cursor_and_erase(){let mut t=terminal();for byte in "é好".as_bytes(){t.feed(&[*byte]).unwrap();}t.feed(b"\rX\x1b[K").unwrap();assert_eq!(text(&mut t),"X\n");}
    #[test] fn bounded_history_and_alternate_screen(){let mut t=terminal();t.feed(b"1\r\n2\r\n3\r\n4\r\n5").unwrap();assert_eq!(text(&mut t),"2\n3\n4\n5");t.feed(b"\x1b[?1049hALT").unwrap();assert_eq!(text(&mut t),"ALT\n");t.feed(b"\x1b[?1049l").unwrap();assert_eq!(text(&mut t),"2\n3\n4\n5");}
    #[test] fn reconfigure_trims_history_and_preserves_partial_utf8() {
        let mut t = terminal(); t.feed(b"1\r\n2\r\n3\r\n4").unwrap();
        t.feed(&[0xc3]).unwrap();
        t.configure(TerminalSpec { columns: 4, rows: 2, scrollback: 0 }).unwrap();
        t.feed(&[0xa9]).unwrap(); assert_eq!(text(&mut t), "3\n4é");
        assert!(t.configure(TerminalSpec { columns: 0, rows: 2, scrollback: 0 }).is_err());
        assert_eq!(text(&mut t), "3\n4é");
    }
    #[test] fn unchanged_dimensions_preserve_pending_wrap_and_scroll_region() {
        let mut t = terminal(); t.feed(b"abcd").unwrap();
        t.configure(TerminalSpec { columns: 4, rows: 2, scrollback: 0 }).unwrap();
        t.feed(b"e").unwrap(); assert_eq!(text(&mut t), "abcd\ne");
        t.feed(b"\x1b[1;2r").unwrap();
        let region = t.screen.region;
        t.configure(TerminalSpec { columns: 4, rows: 2, scrollback: 1 }).unwrap();
        assert_eq!(t.screen.region, region);
    }
}
