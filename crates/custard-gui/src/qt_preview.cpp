// The Qt half of the GTK and Qt page's previews: a few widgets drawn with a
// style (Kvantum, its theme set in $XDG_CONFIG_HOME/Kvantum), offscreen, to
// a PNG. Run in a child process of hyprcustard, never in the app itself:
// it makes a QApplication of its own.

#include <QApplication>
#include <QCheckBox>
#include <QComboBox>
#include <QFont>
#include <QGridLayout>
#include <QHBoxLayout>
#include <QLineEdit>
#include <QListWidget>
#include <QProgressBar>
#include <QPushButton>
#include <QRadioButton>
#include <QSlider>
#include <QStyle>
#include <QStyleFactory>
#include <QTabWidget>
#include <QVBoxLayout>
#include <QWidget>

extern "C" int custard_render_qt_preview(const char* out, const char* style, const char* font)
{
    static int argc = 1;
    static char name[] = "hyprcustard";
    static char* argv[] = { name, nullptr };
    QApplication app(argc, argv);

    QStyle* chosen = QStyleFactory::create(QString::fromUtf8(style));
    if (!chosen)
        return 2;
    QApplication::setStyle(chosen);
    if (font && *font) {
        QFont f;
        if (f.fromString(QString::fromUtf8(font)))
            QApplication::setFont(f);
    }

    QWidget window;
    window.resize(520, 300);
    auto* grid = new QGridLayout(&window);

    auto* tabs = new QTabWidget;
    auto* general = new QWidget;
    tabs->addTab(general, QStringLiteral("General"));
    tabs->addTab(new QWidget, QStringLiteral("Advanced"));
    auto* column = new QVBoxLayout(general);
    auto* buttons = new QHBoxLayout;
    buttons->addWidget(new QPushButton(QStringLiteral("Button")));
    auto* deflt = new QPushButton(QStringLiteral("Default"));
    deflt->setDefault(true);
    buttons->addWidget(deflt);
    column->addLayout(buttons);
    auto* check = new QCheckBox(QStringLiteral("Check box"));
    check->setChecked(true);
    column->addWidget(check);
    auto* radio = new QRadioButton(QStringLiteral("Radio button"));
    radio->setChecked(true);
    column->addWidget(radio);
    column->addWidget(new QLineEdit(QStringLiteral("Text field")));
    auto* combo = new QComboBox;
    combo->addItem(QStringLiteral("Combo box"));
    column->addWidget(combo);
    auto* slider = new QSlider(Qt::Horizontal);
    slider->setValue(60);
    column->addWidget(slider);
    auto* progress = new QProgressBar;
    progress->setValue(45);
    column->addWidget(progress);
    grid->addWidget(tabs, 0, 0);

    auto* list = new QListWidget;
    list->addItems({ QStringLiteral("First item"), QStringLiteral("Selected item"),
                     QStringLiteral("Third item") });
    list->setCurrentRow(1);
    grid->addWidget(list, 0, 1);

    window.setAttribute(Qt::WA_DontShowOnScreen);
    window.show();
    QApplication::processEvents();
    return window.grab().save(QString::fromUtf8(out)) ? 0 : 1;
}
