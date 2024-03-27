use balter_core::ScenarioConfig;

pub enum RuntimeMessage {
    Help(ScenarioConfig),
    Finished,
}
