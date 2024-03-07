use balter_core::config::ScenarioConfig;

pub enum RuntimeMessage {
    Help(ScenarioConfig),
    Finished,
}
