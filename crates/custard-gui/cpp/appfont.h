// The application font, which plain QML Text items take their family from.
//
// cxx-qt-lib can read QGuiApplication's font but not set it, so these two
// small functions do. The font Qt started with — the platform theme's, which
// is qt6ct's or the desktop's — is kept the first time either is called, so
// following the system again puts it back exactly.
#pragma once

#include <QtCore/QString>
#include <QtCore/QStringList>
#include <QtGui/QFont>
#include <QtGui/QGuiApplication>

namespace hyprcustard {

inline const QFont& system_font()
{
    static const QFont font = QGuiApplication::font();
    return font;
}

// An empty family follows the system font.
inline void set_app_font(const QString& family)
{
    QFont font = system_font();
    if (!family.isEmpty()) {
        font.setFamilies(QStringList{ family });
    }
    QGuiApplication::setFont(font);
}

inline QString system_font_family()
{
    return system_font().family();
}

} // namespace hyprcustard
