import com.mojang.math.Transformation;
import org.joml.Quaternionf;
import org.joml.Vector3f;

// Run with the 26.3 Minecraft/JOML classpath. Matrix4f prints rows; Rust fixtures use columns.
class DisplayTransformationOracle {
    public static void main(String[] args) {
        print(new Vector3f(), new Vector3f(1, 0, 0), 0,
            new Vector3f(1), new Vector3f(1, 0, 0), 0);
        print(new Vector3f(1, 2, 3), new Vector3f(4, 5, 6), 0.45f,
            new Vector3f(3), new Vector3f(7, 8, 9), 0.22f);
        print(new Vector3f(-99, 25, 3000), new Vector3f(4, -55, 6), 0.2f,
            new Vector3f(33, 32, 31), new Vector3f(7, 8, -99), -1.24f);
    }

    private static void print(Vector3f translation, Vector3f leftAxis, float leftAngle,
                              Vector3f scale, Vector3f rightAxis, float rightAngle) {
        Transformation value = new Transformation(translation,
            new Quaternionf().fromAxisAngleRad(leftAxis.normalize(), leftAngle), scale,
            new Quaternionf().fromAxisAngleRad(rightAxis.normalize(), rightAngle));
        System.out.println(value.getMatrix());
        Transformation decomposed = new Transformation(value.getMatrix());
        System.out.println(decomposed.leftRotation());
        System.out.println(decomposed.scale());
        System.out.println(decomposed.rightRotation());
    }
}
