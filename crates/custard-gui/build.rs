use cxx_qt_build::{CxxQtBuilder, QmlFile, QmlModule};

fn main() {
    let files = [
        "qml/Main.qml",
        "qml/components/AnimationDemo.qml",
        "qml/components/Badge.qml",
        "qml/components/Card.qml",
        "qml/components/ColorButton.qml",
        "qml/components/ConfirmDialog.qml",
        "qml/components/CurveEditor.qml",
        "qml/components/Dropdown.qml",
        "qml/components/EmptyState.qml",
        "qml/components/FieldRow.qml",
        "qml/components/LabeledSlider.qml",
        "qml/components/LineEdit.qml",
        "qml/components/PasswordDialog.qml",
        "qml/components/PillButton.qml",
        "qml/components/ShaderPicker.qml",
        "qml/components/ShaderPreview.qml",
        "qml/components/Tip.qml",
        "qml/components/ThemePreview.qml",
        "qml/components/Toast.qml",
        "qml/components/Toggle.qml",
        "qml/pages/ActionEditor.qml",
        "qml/pages/AnimationPage.qml",
        "qml/pages/BindsPage.qml",
        "qml/pages/LayersPage.qml",
        "qml/pages/LookOptionRow.qml",
        "qml/pages/LookPage.qml",
        "qml/pages/ParamEditor.qml",
        "qml/pages/PluginPage.qml",
        "qml/pages/PreviewPage.qml",
        "qml/pages/RulesPage.qml",
        "qml/pages/SettingsPage.qml",
        "qml/pages/ShadersPage.qml",
        "qml/pages/ThemesPage.qml",
    ];

    let module = QmlModule::new("dev.hyprcustard.gui")
        // Theme and App are QML singletons: every component reads colours and
        // state from them rather than having them threaded down by hand.
        .qml_file(QmlFile::from("qml/Theme.qml").singleton(true))
        .qml_file(QmlFile::from("qml/App.qml").singleton(true))
        .qml_files(files);

    CxxQtBuilder::new_qml_module(module)
        .qt_module("Quick")
        .qt_module("QuickControls2")
        .files(["src/bridge.rs"])
        .build();
}
