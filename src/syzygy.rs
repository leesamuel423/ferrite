pub struct SyzygyProber;
    impl SyzygyProber {
        pub fn new(_path: &str) -> Option<Self> { None }
        pub fn probe_wdl(&self, _board: &crate::board::Board) -> Option<crate::types::Score> { 
            None 
        }
}

